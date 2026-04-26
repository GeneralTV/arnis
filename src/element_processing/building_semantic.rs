//! Automatic semantic classification for OSM building / amenity / shop /
//! station objects.
//!
//! The renderer in `buildings.rs` historically picked a coarse
//! [`super::buildings::BuildingCategory`] from a small set of `building=`
//! tag values. That misses a huge class of "the OSM tag says
//! `building=service` but `amenity=fuel` and `brand=Лукойл`, this is
//! obviously a gas station, not a generic service shed".
//!
//! This module is the first half of that fix: a richer
//! [`BuildingSubtype`] enum (≈120 functional building types matching
//! the spec the user provided) plus a [`classify`] function that maps
//! `(tags, name, brand, operator, footprint, neighbours)` to one of
//! those subtypes.
//!
//! The output is stored on each building's [`BuildingSemantic`] before
//! rendering. For now the renderer in `buildings.rs` consumes only the
//! coarse category, so this module is **purely classification**: it
//! does not change generated blocks. Subsequent PRs (gas station,
//! school, station, brand theming, …) plug in subtype-specific render
//! paths and consume the richer information.
//!
//! The classifier is intentionally tag-driven and deterministic. There
//! is no RNG and no probabilistic model — each subtype is the result of
//! a chain of `match` checks, in priority order.

// Skeleton module — subsequent PRs (gas station, school, station,
// brand theming, …) consume these symbols. Until then they are
// reachable only from the unit tests in this file.
#![allow(dead_code)]

use crate::osm_parser::ProcessedWay;
use std::collections::HashMap;

/// Functional sub-category of a building, derived from OSM semantic
/// tags rather than just the raw `building=` value.
///
/// Each variant is a "kind of place" that should *look* a specific way
/// in the rendered world (a gas station has a canopy + pumps, a school
/// has a yard + multiple wings, a metro entrance has stairs going down,
/// etc.). The variants are intentionally fine-grained so brand /
/// operator theming and category-specific interiors have a single
/// place to dispatch on.
///
/// Mapping back to the renderer's coarser
/// [`super::buildings::BuildingCategory`] is via
/// [`BuildingSubtype::coarse_category`]; renderers that don't yet have
/// subtype-specific code keep using the coarse value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum BuildingSubtype {
    // ---------- Residential ----------
    DetachedHouse,
    SemiDetachedHouse,
    Townhouse,
    Cottage,
    RuralHouse,
    Apartments,
    ResidentialBlock,
    Dormitory,
    Hostel,
    MixedUseResidential,

    // ---------- Education ----------
    Kindergarten,
    School,
    College,
    University,
    TrainingCenter,
    Library,

    // ---------- Healthcare ----------
    Hospital,
    Clinic,
    Polyclinic,
    MedicalCenter,
    Pharmacy,
    DentalClinic,
    VeterinaryClinic,

    // ---------- Retail ----------
    ConvenienceStore,
    Minimarket,
    Supermarket,
    Hypermarket,
    ShoppingMall,
    RetailStore,
    LiquorStore,
    PharmacyStore,
    Bakery,
    Butcher,
    Greengrocer,
    ElectronicsStore,
    HardwareStore,
    FurnitureStore,
    ClothingStore,
    BeautyStore,
    HouseholdStore,
    DiscountStore,
    PickupPoint,
    MarketHall,
    Kiosk,

    // ---------- Food service ----------
    Cafe,
    CoffeeShop,
    Restaurant,
    FastFood,
    BurgerRestaurant,
    PizzaRestaurant,
    Canteen,
    FoodCourt,
    BakeryCafe,

    // ---------- Office / civic ----------
    Office,
    BusinessCenter,
    Bank,
    PostOffice,
    GovernmentOffice,
    MunicipalBuilding,
    PoliceStation,
    FireStation,
    Courthouse,
    SocialServiceCenter,

    // ---------- Transport ----------
    GasStation,
    CarWash,
    CarService,
    Dealership,
    ParkingGarage,
    BusStation,
    BusTerminal,
    RailwayStation,
    RailwayHalt,
    MetroStation,
    MetroEntrance,
    TramStation,
    TransportHub,
    AirportTerminal,

    // ---------- Industrial ----------
    Warehouse,
    IndustrialBuilding,
    Factory,
    Workshop,
    Depot,
    LogisticsCenter,
    UtilityBuilding,
    PowerFacility,
    WaterTreatment,
    TransformerStation,

    // ---------- Sport / culture ----------
    SportsHall,
    Stadium,
    SwimmingPool,
    FitnessCenter,
    Cinema,
    Theater,
    Museum,
    ExhibitionHall,
    CulturalCenter,
    YouthCenter,

    // ---------- Religious ----------
    Church,
    Chapel,
    Mosque,
    Synagogue,
    Temple,
    Monastery,

    // ---------- Hospitality ----------
    Hotel,
    Motel,
    GuestHouse,
    ResortBuilding,

    // ---------- Agricultural ----------
    Barn,
    FarmBuilding,
    Stable,
    Greenhouse,
    StorageShed,

    // ---------- Special ----------
    PublicToilet,
    Pavilion,
    ServiceBuilding,
    AbandonedBuilding,
    HistoricBuilding,
    MonumentSupportBuilding,
    UnknownPublic,
    UnknownCommercial,
    UnknownIndustrial,
    Unknown,
}

impl BuildingSubtype {
    /// Best-fit coarse category for this subtype. Used while subtype-
    /// specific renderers are still being added: subtypes that don't
    /// have a dedicated render path fall back to the coarse category's
    /// existing renderer. Strings match the variants of
    /// `super::buildings::BuildingCategory` so it can be used in a
    /// `match` directly.
    #[allow(dead_code)]
    pub fn coarse_category_str(&self) -> &'static str {
        use BuildingSubtype as S;
        match self {
            // Residential
            S::DetachedHouse | S::SemiDetachedHouse | S::Townhouse | S::Cottage | S::RuralHouse => {
                "House"
            }
            S::Apartments
            | S::ResidentialBlock
            | S::Dormitory
            | S::Hostel
            | S::MixedUseResidential => "Residential",

            // Education / civic / health → School / Hospital coarse buckets
            S::Kindergarten
            | S::School
            | S::College
            | S::University
            | S::TrainingCenter
            | S::Library => "School",
            S::Hospital
            | S::Clinic
            | S::Polyclinic
            | S::MedicalCenter
            | S::DentalClinic
            | S::VeterinaryClinic => "Hospital",

            // Retail / food → Commercial
            S::ConvenienceStore
            | S::Minimarket
            | S::Supermarket
            | S::Hypermarket
            | S::ShoppingMall
            | S::RetailStore
            | S::LiquorStore
            | S::Pharmacy
            | S::PharmacyStore
            | S::Bakery
            | S::Butcher
            | S::Greengrocer
            | S::ElectronicsStore
            | S::HardwareStore
            | S::FurnitureStore
            | S::ClothingStore
            | S::BeautyStore
            | S::HouseholdStore
            | S::DiscountStore
            | S::PickupPoint
            | S::MarketHall
            | S::Cafe
            | S::CoffeeShop
            | S::Restaurant
            | S::FastFood
            | S::BurgerRestaurant
            | S::PizzaRestaurant
            | S::Canteen
            | S::FoodCourt
            | S::BakeryCafe
            | S::Kiosk => "Commercial",

            // Office / civic / government / banks
            S::Office
            | S::BusinessCenter
            | S::Bank
            | S::PostOffice
            | S::GovernmentOffice
            | S::MunicipalBuilding
            | S::PoliceStation
            | S::FireStation
            | S::Courthouse
            | S::SocialServiceCenter => "Office",

            // Hotels
            S::Hotel | S::Motel | S::GuestHouse | S::ResortBuilding => "Hotel",

            // Transport — most are public-civic-feeling, station-shaped
            // buildings; coarse-mapped to Office for now until a subtype
            // renderer takes over.
            S::GasStation
            | S::CarWash
            | S::CarService
            | S::Dealership
            | S::ParkingGarage
            | S::BusStation
            | S::BusTerminal
            | S::RailwayStation
            | S::RailwayHalt
            | S::MetroStation
            | S::MetroEntrance
            | S::TramStation
            | S::TransportHub
            | S::AirportTerminal => "Office",

            // Industrial
            S::IndustrialBuilding
            | S::Factory
            | S::Workshop
            | S::UtilityBuilding
            | S::PowerFacility
            | S::WaterTreatment
            | S::TransformerStation
            | S::UnknownIndustrial => "Industrial",
            S::Warehouse | S::Depot | S::LogisticsCenter => "Warehouse",

            // Sport / culture → Office bucket today (large public box)
            S::SportsHall
            | S::Stadium
            | S::SwimmingPool
            | S::FitnessCenter
            | S::Cinema
            | S::Theater
            | S::Museum
            | S::ExhibitionHall
            | S::CulturalCenter
            | S::YouthCenter => "Office",

            // Religion
            S::Church | S::Chapel | S::Mosque | S::Synagogue | S::Temple | S::Monastery => {
                "Religious"
            }

            // Agricultural
            S::Barn | S::FarmBuilding | S::Stable | S::StorageShed => "Farm",
            S::Greenhouse => "Greenhouse",

            // Special
            S::PublicToilet | S::Pavilion | S::ServiceBuilding | S::UnknownPublic => "Default",
            S::AbandonedBuilding | S::HistoricBuilding | S::MonumentSupportBuilding => "Historic",
            S::UnknownCommercial => "Commercial",
            S::Unknown => "Default",
        }
    }
}

/// Result of automatic semantic classification of an OSM way.
///
/// Stored alongside building geometry; consumed by subtype-aware
/// renderers as they are added. The renderer pipeline never *requires*
/// this struct — anything missing falls back to the coarse-category
/// path that already exists.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BuildingSemantic {
    pub subtype: BuildingSubtype,
    /// 0.0–1.0 — how strongly the inputs agreed on the chosen subtype.
    /// 1.0 = unambiguous (`amenity=fuel` → `GasStation`),
    /// ≤0.5 = inferred from weaker signals or footprint heuristics.
    pub confidence: f32,
    /// Normalised brand identifier (lowercase, ASCII transliteration of
    /// Cyrillic where applicable). `None` when no recognised brand was
    /// found. Used by the brand-theming PR to look up colour palettes.
    pub brand_key: Option<String>,
    /// Raw values that drove the classification, for `--debug-building-specs`.
    pub detected_from: DetectedFrom,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DetectedFrom {
    pub primary_tag: Option<(String, String)>,
    pub name: Option<String>,
    pub brand: Option<String>,
    pub operator: Option<String>,
    pub context_clues: Vec<String>,
}

/// Classify an OSM way into a [`BuildingSubtype`] using its tags,
/// name/brand/operator, and (when available) lightweight context.
///
/// Decision order, highest priority first:
///
/// 1. Explicit transport-infrastructure tags (`amenity=fuel`,
///    `railway=station`, `railway=subway_entrance`, `aeroway=terminal`).
///    These trump the `building=` value because the OSM convention is to
///    use `building=service` / `building=yes` even when the function is
///    obvious.
/// 2. Explicit civic / health / education tags (`amenity=hospital`,
///    `amenity=school`, …).
/// 3. `shop=*` and `amenity=*` retail / food values, with brand boost.
/// 4. `building=*` value itself.
/// 5. Footprint-only fallbacks (very long narrow building → likely
///    barn / depot).
/// 6. `Unknown`.
pub fn classify(way: &ProcessedWay) -> BuildingSemantic {
    let tags = &way.tags;
    let brand_raw = tags.get("brand").or_else(|| tags.get("operator")).cloned();
    let brand_key = brand_raw.as_deref().map(normalize_brand);
    let name = tags.get("name").cloned();
    let operator = tags.get("operator").cloned();
    let mut detected = DetectedFrom {
        name: name.clone(),
        brand: brand_raw.clone(),
        operator: operator.clone(),
        ..Default::default()
    };

    if let Some((sub, primary, conf)) = classify_inner(tags, brand_key.as_deref(), &mut detected) {
        detected.primary_tag = Some(primary);
        BuildingSemantic {
            subtype: sub,
            confidence: conf,
            brand_key,
            detected_from: detected,
        }
    } else {
        BuildingSemantic {
            subtype: BuildingSubtype::Unknown,
            confidence: 0.0,
            brand_key,
            detected_from: detected,
        }
    }
}

fn classify_inner(
    tags: &HashMap<String, String>,
    brand_key: Option<&str>,
    detected: &mut DetectedFrom,
) -> Option<(BuildingSubtype, (String, String), f32)> {
    use BuildingSubtype as S;

    // ---- 1. Transport infrastructure (overrides building=) ----
    if let Some(v) = tags.get("amenity") {
        match v.as_str() {
            "fuel" => return Some((S::GasStation, ("amenity".into(), v.clone()), 1.0)),
            "car_wash" => return Some((S::CarWash, ("amenity".into(), v.clone()), 1.0)),
            "parking" | "parking_entrance"
                if tags.get("parking").map(|s| s.as_str()) == Some("multi-storey")
                    || tags.get("building").map(|s| s.as_str()) == Some("parking") =>
            {
                return Some((S::ParkingGarage, ("amenity".into(), v.clone()), 0.9));
            }
            "bus_station" => return Some((S::BusStation, ("amenity".into(), v.clone()), 1.0)),
            _ => {}
        }
    }
    if let Some(v) = tags.get("railway") {
        match v.as_str() {
            "station" => {
                let is_subway = tags.get("station").map(|s| s.as_str()) == Some("subway")
                    || tags.get("subway").map(|s| s.as_str()) == Some("yes");
                let is_tram = tags.get("tram").map(|s| s.as_str()) == Some("yes")
                    || tags.get("station").map(|s| s.as_str()) == Some("tram");
                if is_subway {
                    return Some((S::MetroStation, ("railway".into(), v.clone()), 1.0));
                }
                if is_tram {
                    return Some((S::TramStation, ("railway".into(), v.clone()), 1.0));
                }
                return Some((S::RailwayStation, ("railway".into(), v.clone()), 1.0));
            }
            "halt" => return Some((S::RailwayHalt, ("railway".into(), v.clone()), 1.0)),
            "subway_entrance" => {
                return Some((S::MetroEntrance, ("railway".into(), v.clone()), 1.0));
            }
            "tram_stop" => return Some((S::TramStation, ("railway".into(), v.clone()), 0.85)),
            _ => {}
        }
    }
    if tags.get("aeroway").map(|s| s.as_str()) == Some("terminal") {
        return Some((
            S::AirportTerminal,
            ("aeroway".into(), "terminal".into()),
            1.0,
        ));
    }
    if tags.get("public_transport").map(|s| s.as_str()) == Some("station") {
        return Some((
            S::TransportHub,
            ("public_transport".into(), "station".into()),
            0.85,
        ));
    }

    // ---- 2. Civic / health / education ----
    if let Some(v) = tags.get("amenity") {
        match v.as_str() {
            "hospital" => return Some((S::Hospital, ("amenity".into(), v.clone()), 1.0)),
            "clinic" => return Some((S::Clinic, ("amenity".into(), v.clone()), 1.0)),
            "doctors" => return Some((S::MedicalCenter, ("amenity".into(), v.clone()), 0.9)),
            "dentist" => return Some((S::DentalClinic, ("amenity".into(), v.clone()), 1.0)),
            "veterinary" => return Some((S::VeterinaryClinic, ("amenity".into(), v.clone()), 1.0)),
            "pharmacy" => return Some((S::Pharmacy, ("amenity".into(), v.clone()), 1.0)),
            "school" => return Some((S::School, ("amenity".into(), v.clone()), 1.0)),
            "kindergarten" | "childcare" => {
                return Some((S::Kindergarten, ("amenity".into(), v.clone()), 1.0));
            }
            "college" => return Some((S::College, ("amenity".into(), v.clone()), 1.0)),
            "university" => return Some((S::University, ("amenity".into(), v.clone()), 1.0)),
            "library" => return Some((S::Library, ("amenity".into(), v.clone()), 1.0)),
            "training" | "driving_school" | "language_school" | "music_school" => {
                return Some((S::TrainingCenter, ("amenity".into(), v.clone()), 0.85));
            }
            "police" => return Some((S::PoliceStation, ("amenity".into(), v.clone()), 1.0)),
            "fire_station" => {
                return Some((S::FireStation, ("amenity".into(), v.clone()), 1.0));
            }
            "courthouse" => return Some((S::Courthouse, ("amenity".into(), v.clone()), 1.0)),
            "townhall" => {
                return Some((S::MunicipalBuilding, ("amenity".into(), v.clone()), 1.0));
            }
            "post_office" => return Some((S::PostOffice, ("amenity".into(), v.clone()), 1.0)),
            "bank" | "bureau_de_change" => {
                return Some((S::Bank, ("amenity".into(), v.clone()), 1.0));
            }
            "social_facility" | "social_centre" => {
                return Some((S::SocialServiceCenter, ("amenity".into(), v.clone()), 0.85));
            }
            "place_of_worship" => {
                let religion = tags.get("religion").map(|s| s.as_str()).unwrap_or("");
                let denom = tags.get("denomination").map(|s| s.as_str()).unwrap_or("");
                return Some(match religion {
                    "muslim" => (S::Mosque, ("amenity".into(), v.clone()), 1.0),
                    "jewish" => (S::Synagogue, ("amenity".into(), v.clone()), 1.0),
                    "buddhist" | "hindu" | "shinto" | "taoist" => {
                        (S::Temple, ("amenity".into(), v.clone()), 1.0)
                    }
                    "christian" if denom == "monastic" => {
                        (S::Monastery, ("amenity".into(), v.clone()), 0.9)
                    }
                    "christian" => (S::Church, ("amenity".into(), v.clone()), 1.0),
                    _ => (S::Church, ("amenity".into(), v.clone()), 0.6),
                });
            }
            "cinema" => return Some((S::Cinema, ("amenity".into(), v.clone()), 1.0)),
            "theatre" => return Some((S::Theater, ("amenity".into(), v.clone()), 1.0)),
            "arts_centre" | "community_centre" => {
                return Some((S::CulturalCenter, ("amenity".into(), v.clone()), 0.9));
            }
            "exhibition_centre" => {
                return Some((S::ExhibitionHall, ("amenity".into(), v.clone()), 1.0));
            }
            "toilets" => return Some((S::PublicToilet, ("amenity".into(), v.clone()), 1.0)),
            // food
            "cafe" => {
                if brand_is_coffee_chain(brand_key) {
                    return Some((S::CoffeeShop, ("amenity".into(), v.clone()), 0.95));
                }
                return Some((S::Cafe, ("amenity".into(), v.clone()), 0.95));
            }
            "restaurant" => {
                if brand_is_burger_chain(brand_key) {
                    return Some((S::BurgerRestaurant, ("amenity".into(), v.clone()), 1.0));
                }
                if brand_is_pizza_chain(brand_key) {
                    return Some((S::PizzaRestaurant, ("amenity".into(), v.clone()), 1.0));
                }
                return Some((S::Restaurant, ("amenity".into(), v.clone()), 0.95));
            }
            "fast_food" => {
                if brand_is_burger_chain(brand_key) {
                    return Some((S::BurgerRestaurant, ("amenity".into(), v.clone()), 1.0));
                }
                if brand_is_pizza_chain(brand_key) {
                    return Some((S::PizzaRestaurant, ("amenity".into(), v.clone()), 1.0));
                }
                return Some((S::FastFood, ("amenity".into(), v.clone()), 1.0));
            }
            "food_court" => return Some((S::FoodCourt, ("amenity".into(), v.clone()), 1.0)),
            "bar" | "pub" => return Some((S::Restaurant, ("amenity".into(), v.clone()), 0.7)),
            _ => {}
        }
    }

    // ---- 3. Tourism / leisure / office ----
    if let Some(v) = tags.get("tourism") {
        match v.as_str() {
            "hotel" => return Some((S::Hotel, ("tourism".into(), v.clone()), 1.0)),
            "motel" => return Some((S::Motel, ("tourism".into(), v.clone()), 1.0)),
            "guest_house" => return Some((S::GuestHouse, ("tourism".into(), v.clone()), 1.0)),
            "hostel" => return Some((S::Hostel, ("tourism".into(), v.clone()), 1.0)),
            "museum" => return Some((S::Museum, ("tourism".into(), v.clone()), 1.0)),
            _ => {}
        }
    }
    if let Some(v) = tags.get("leisure") {
        match v.as_str() {
            "sports_centre" | "sports_hall" => {
                return Some((S::SportsHall, ("leisure".into(), v.clone()), 1.0));
            }
            "stadium" => return Some((S::Stadium, ("leisure".into(), v.clone()), 1.0)),
            "swimming_pool" => return Some((S::SwimmingPool, ("leisure".into(), v.clone()), 0.95)),
            "fitness_centre" => {
                return Some((S::FitnessCenter, ("leisure".into(), v.clone()), 1.0));
            }
            _ => {}
        }
    }
    if tags.contains_key("office") {
        return Some((S::Office, ("office".into(), "*".into()), 0.9));
    }

    // ---- 4. Shop / retail ----
    if let Some(v) = tags.get("shop") {
        let sub = match v.as_str() {
            "supermarket" => {
                if brand_is_hypermarket(brand_key) {
                    S::Hypermarket
                } else {
                    S::Supermarket
                }
            }
            "convenience" => S::ConvenienceStore,
            "mall" | "department_store" => S::ShoppingMall,
            "kiosk" => S::Kiosk,
            "alcohol" | "wine" | "beverages" => S::LiquorStore,
            "bakery" => S::Bakery,
            "butcher" => S::Butcher,
            "greengrocer" => S::Greengrocer,
            "electronics" | "computer" | "mobile_phone" => S::ElectronicsStore,
            "hardware" | "doityourself" | "trade" => S::HardwareStore,
            "furniture" => S::FurnitureStore,
            "clothes" | "fashion" | "shoes" => S::ClothingStore,
            "cosmetics" | "perfumery" | "beauty" => S::BeautyStore,
            "household" | "houseware" | "variety_store" => S::HouseholdStore,
            "discount" => S::DiscountStore,
            "general" | "yes" => S::RetailStore,
            "marketplace" | "market" => S::MarketHall,
            _ => S::RetailStore,
        };
        return Some((sub, ("shop".into(), v.clone()), 0.95));
    }
    if tags.get("amenity").map(|s| s.as_str()) == Some("marketplace") {
        return Some((
            S::MarketHall,
            ("amenity".into(), "marketplace".into()),
            0.95,
        ));
    }
    if tags.get("amenity").map(|s| s.as_str()) == Some("vending_machine")
        || tags.get("amenity").map(|s| s.as_str()) == Some("parcel_locker")
    {
        return Some((
            S::PickupPoint,
            ("amenity".into(), "parcel_locker".into()),
            0.9,
        ));
    }

    // ---- 5. Healthcare wing (tagged via healthcare=*) ----
    if let Some(v) = tags.get("healthcare") {
        let sub = match v.as_str() {
            "hospital" => S::Hospital,
            "clinic" => S::Clinic,
            "pharmacy" => S::Pharmacy,
            "dentist" => S::DentalClinic,
            "veterinary" => S::VeterinaryClinic,
            _ => S::MedicalCenter,
        };
        return Some((sub, ("healthcare".into(), v.clone()), 0.95));
    }

    // ---- 6. Industrial / utility ----
    if let Some(v) = tags.get("man_made") {
        match v.as_str() {
            "works" => {
                return Some((S::Factory, ("man_made".into(), v.clone()), 0.9));
            }
            "wastewater_plant" | "water_works" => {
                return Some((S::WaterTreatment, ("man_made".into(), v.clone()), 1.0));
            }
            "transformer_station" => {
                return Some((S::TransformerStation, ("man_made".into(), v.clone()), 1.0));
            }
            _ => {}
        }
    }
    if tags.get("power").map(|s| s.as_str()) == Some("substation")
        || tags.get("power").map(|s| s.as_str()) == Some("plant")
    {
        return Some((S::PowerFacility, ("power".into(), "*".into()), 1.0));
    }
    if let Some(v) = tags.get("industrial") {
        match v.as_str() {
            "factory" => return Some((S::Factory, ("industrial".into(), v.clone()), 0.95)),
            "warehouse" => return Some((S::Warehouse, ("industrial".into(), v.clone()), 0.95)),
            "depot" => return Some((S::Depot, ("industrial".into(), v.clone()), 0.95)),
            _ => {
                return Some((
                    S::IndustrialBuilding,
                    ("industrial".into(), v.clone()),
                    0.85,
                ));
            }
        }
    }

    // ---- 7. Religion (building=church without amenity=place_of_worship) ----
    let building_type = tags
        .get("building")
        .or_else(|| tags.get("building:part"))
        .map(|s| s.as_str())
        .unwrap_or("yes");
    match building_type {
        "church" | "cathedral" => {
            return Some((S::Church, ("building".into(), building_type.into()), 1.0));
        }
        "chapel" => return Some((S::Chapel, ("building".into(), building_type.into()), 1.0)),
        "mosque" => return Some((S::Mosque, ("building".into(), building_type.into()), 1.0)),
        "synagogue" => return Some((S::Synagogue, ("building".into(), building_type.into()), 1.0)),
        "temple" => return Some((S::Temple, ("building".into(), building_type.into()), 1.0)),
        "monastery" => return Some((S::Monastery, ("building".into(), building_type.into()), 1.0)),
        _ => {}
    }

    // ---- 8. Building= residential bucket ----
    match building_type {
        "house" => return Some((S::DetachedHouse, ("building".into(), "house".into()), 0.95)),
        "detached" => {
            return Some((
                S::DetachedHouse,
                ("building".into(), "detached".into()),
                1.0,
            ));
        }
        "semidetached_house" => {
            return Some((
                S::SemiDetachedHouse,
                ("building".into(), building_type.into()),
                1.0,
            ));
        }
        "terrace" => return Some((S::Townhouse, ("building".into(), "terrace".into()), 1.0)),
        "bungalow" | "cottage" => {
            return Some((S::Cottage, ("building".into(), building_type.into()), 1.0));
        }
        "cabin" | "hut" | "static_caravan" => {
            return Some((
                S::RuralHouse,
                ("building".into(), building_type.into()),
                0.85,
            ));
        }
        "apartments" => {
            return Some((S::Apartments, ("building".into(), "apartments".into()), 1.0));
        }
        "residential" => {
            return Some((
                S::ResidentialBlock,
                ("building".into(), "residential".into()),
                0.95,
            ));
        }
        "dormitory" => {
            return Some((S::Dormitory, ("building".into(), "dormitory".into()), 1.0));
        }
        "hostel" => return Some((S::Hostel, ("building".into(), "hostel".into()), 1.0)),
        // Industrial / agricultural / utility values
        "industrial" | "manufacture" => {
            return Some((
                S::IndustrialBuilding,
                ("building".into(), building_type.into()),
                0.95,
            ));
        }
        "factory" => {
            return Some((S::Factory, ("building".into(), "factory".into()), 1.0));
        }
        "warehouse" => {
            return Some((S::Warehouse, ("building".into(), "warehouse".into()), 1.0));
        }
        "hangar" => return Some((S::Depot, ("building".into(), "hangar".into()), 0.85)),
        "barn" => return Some((S::Barn, ("building".into(), "barn".into()), 1.0)),
        "stable" | "cowshed" | "sty" | "sheepfold" => {
            return Some((S::Stable, ("building".into(), building_type.into()), 0.95));
        }
        "farm" | "farm_auxiliary" => {
            return Some((
                S::FarmBuilding,
                ("building".into(), building_type.into()),
                0.95,
            ));
        }
        "greenhouse" | "glasshouse" => {
            return Some((
                S::Greenhouse,
                ("building".into(), building_type.into()),
                1.0,
            ));
        }
        "shed" => {
            return Some((S::StorageShed, ("building".into(), "shed".into()), 0.85));
        }
        "garage" | "garages" | "carport" => {
            return Some((
                S::CarService,
                ("building".into(), building_type.into()),
                0.6,
            ));
        }
        "school" | "kindergarten" => {
            let s = if building_type == "kindergarten" {
                S::Kindergarten
            } else {
                S::School
            };
            return Some((s, ("building".into(), building_type.into()), 1.0));
        }
        "college" => {
            return Some((S::College, ("building".into(), "college".into()), 1.0));
        }
        "university" => {
            return Some((S::University, ("building".into(), "university".into()), 1.0));
        }
        "hospital" => {
            return Some((S::Hospital, ("building".into(), "hospital".into()), 1.0));
        }
        "hotel" => return Some((S::Hotel, ("building".into(), "hotel".into()), 1.0)),
        "motel" => return Some((S::Motel, ("building".into(), "motel".into()), 1.0)),
        "office" => return Some((S::Office, ("building".into(), "office".into()), 1.0)),
        "commercial" | "retail" | "supermarket" | "shop" => {
            // Without a more specific tag, treat as generic retail.
            let sub = if building_type == "supermarket" {
                S::Supermarket
            } else {
                S::RetailStore
            };
            return Some((sub, ("building".into(), building_type.into()), 0.8));
        }
        "kiosk" => return Some((S::Kiosk, ("building".into(), "kiosk".into()), 1.0)),
        "transportation" | "train_station" => {
            return Some((
                S::RailwayStation,
                ("building".into(), building_type.into()),
                0.9,
            ));
        }
        "service" => {
            // 'service' is the catch-all OSM uses for "small functional
            // building" — sometimes a fuel station shop, sometimes a
            // tool shed. Without amenity=fuel etc. above, drop to
            // ServiceBuilding rather than House.
            return Some((
                S::ServiceBuilding,
                ("building".into(), "service".into()),
                0.6,
            ));
        }
        "construction" | "ruins" => {
            return Some((
                S::AbandonedBuilding,
                ("building".into(), building_type.into()),
                0.9,
            ));
        }
        "castle" | "fort" | "bunker" => {
            return Some((
                S::HistoricBuilding,
                ("building".into(), building_type.into()),
                1.0,
            ));
        }
        "tower" | "clock_tower" | "transformer_tower" => {
            return Some((
                S::MonumentSupportBuilding,
                ("building".into(), building_type.into()),
                0.9,
            ));
        }
        "public" | "civic" | "government" => {
            return Some((
                S::GovernmentOffice,
                ("building".into(), building_type.into()),
                0.85,
            ));
        }
        "yes" => {
            // No useful semantic. Fall back below.
            detected.context_clues.push("building=yes".into());
        }
        _ => {}
    }

    // ---- 9. Historic ----
    if tags.contains_key("historic") {
        return Some((
            S::HistoricBuilding,
            (
                "historic".into(),
                tags.get("historic").cloned().unwrap_or_default(),
            ),
            0.85,
        ));
    }

    // ---- 10. Last-resort: building=yes with unknown function ----
    Some((S::Unknown, ("building".into(), "yes".into()), 0.2))
}

/// Normalise brand / operator strings to a stable lookup key.
///
/// Lowercases, strips punctuation, transliterates the most common
/// Cyrillic letters used in Russian retail brand names. Designed to
/// canonicalise variants like "Красное и Белое" / "Красное & Белое" /
/// "Красное&Белое" / "K&B" → `krasnoe_beloe`.
pub fn normalize_brand(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut prev_underscore = false;
    for ch in lower.chars() {
        let mapped: Option<&str> = match ch {
            'а' => Some("a"),
            'б' => Some("b"),
            'в' => Some("v"),
            'г' => Some("g"),
            'д' => Some("d"),
            'е' | 'ё' => Some("e"),
            'ж' => Some("zh"),
            'з' => Some("z"),
            'и' | 'й' => Some("i"),
            'к' => Some("k"),
            'л' => Some("l"),
            'м' => Some("m"),
            'н' => Some("n"),
            'о' => Some("o"),
            'п' => Some("p"),
            'р' => Some("r"),
            'с' => Some("s"),
            'т' => Some("t"),
            'у' => Some("u"),
            'ф' => Some("f"),
            'х' => Some("h"),
            'ц' => Some("ts"),
            'ч' => Some("ch"),
            'ш' => Some("sh"),
            'щ' => Some("sch"),
            'ъ' | 'ь' => Some(""),
            'ы' => Some("y"),
            'э' => Some("e"),
            'ю' => Some("yu"),
            'я' => Some("ya"),
            _ => None,
        };
        if let Some(s) = mapped {
            for c in s.chars() {
                out.push(c);
                prev_underscore = false;
            }
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Brand families used to cross-validate `shop=supermarket` vs
/// `shop=hypermarket` (OSM tagging is inconsistent here, so we can
/// promote based on known brand size).
fn brand_is_hypermarket(brand: Option<&str>) -> bool {
    let Some(b) = brand else {
        return false;
    };
    matches!(
        b,
        "auchan"
            | "ashan"
            | "lenta"
            | "okay"
            | "okei"
            | "metro"
            | "selgros"
            | "globus"
            | "carrefour"
            | "tesco"
            | "walmart"
            | "costco"
    )
}

fn brand_is_burger_chain(brand: Option<&str>) -> bool {
    let Some(b) = brand else {
        return false;
    };
    matches!(
        b,
        "burger_king"
            | "kfc"
            | "rostic_s"
            | "rostics"
            | "vkusno_i_tochka"
            | "mcdonald_s"
            | "mcdonalds"
            | "wendys"
            | "wendy_s"
    )
}

fn brand_is_pizza_chain(brand: Option<&str>) -> bool {
    let Some(b) = brand else {
        return false;
    };
    matches!(
        b,
        "dodo_pizza"
            | "papa_john_s"
            | "papa_johns"
            | "domino_s_pizza"
            | "dominos_pizza"
            | "pizza_hut"
    )
}

fn brand_is_coffee_chain(brand: Option<&str>) -> bool {
    let Some(b) = brand else {
        return false;
    };
    matches!(
        b,
        "starbucks"
            | "stars_coffee"
            | "coffix"
            | "shokoladnitsa"
            | "kofeinya"
            | "cofix"
            | "tim_hortons"
            | "costa_coffee"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_parser::{ProcessedNode, ProcessedWay};

    fn way_with_tags(pairs: &[(&str, &str)]) -> ProcessedWay {
        let mut tags = HashMap::new();
        for (k, v) in pairs {
            tags.insert((*k).to_string(), (*v).to_string());
        }
        ProcessedWay {
            id: 1,
            nodes: vec![ProcessedNode {
                id: 1,
                tags: HashMap::new(),
                x: 0,
                z: 0,
            }],
            tags,
        }
    }

    #[test]
    fn fuel_station_overrides_building_service() {
        let w = way_with_tags(&[
            ("building", "service"),
            ("amenity", "fuel"),
            ("brand", "Лукойл"),
        ]);
        let s = classify(&w);
        assert_eq!(s.subtype, BuildingSubtype::GasStation);
        assert_eq!(s.brand_key.as_deref(), Some("lukoil"));
    }

    #[test]
    fn supermarket_brand_promotes_to_hypermarket() {
        let w = way_with_tags(&[("shop", "supermarket"), ("brand", "Ашан")]);
        let s = classify(&w);
        assert_eq!(s.subtype, BuildingSubtype::Hypermarket);
    }

    #[test]
    fn pyaterochka_stays_supermarket() {
        let w = way_with_tags(&[("shop", "supermarket"), ("brand", "Пятёрочка")]);
        let s = classify(&w);
        assert_eq!(s.subtype, BuildingSubtype::Supermarket);
        assert_eq!(s.brand_key.as_deref(), Some("pyaterochka"));
    }

    #[test]
    fn kindergarten_school_university() {
        for (val, expected) in [
            ("kindergarten", BuildingSubtype::Kindergarten),
            ("school", BuildingSubtype::School),
            ("university", BuildingSubtype::University),
        ] {
            let w = way_with_tags(&[("amenity", val)]);
            assert_eq!(classify(&w).subtype, expected);
        }
    }

    #[test]
    fn subway_entrance_classified_as_metro() {
        let w = way_with_tags(&[("railway", "subway_entrance"), ("name", "Котельники")]);
        let s = classify(&w);
        assert_eq!(s.subtype, BuildingSubtype::MetroEntrance);
    }

    #[test]
    fn place_of_worship_dispatches_on_religion() {
        for (relig, expected) in [
            ("muslim", BuildingSubtype::Mosque),
            ("jewish", BuildingSubtype::Synagogue),
            ("buddhist", BuildingSubtype::Temple),
            ("christian", BuildingSubtype::Church),
        ] {
            let w = way_with_tags(&[("amenity", "place_of_worship"), ("religion", relig)]);
            assert_eq!(classify(&w).subtype, expected);
        }
    }

    #[test]
    fn yes_building_is_unknown_low_confidence() {
        let w = way_with_tags(&[("building", "yes")]);
        let s = classify(&w);
        assert_eq!(s.subtype, BuildingSubtype::Unknown);
        assert!(s.confidence < 0.5);
    }

    #[test]
    fn brand_normalisation() {
        assert_eq!(normalize_brand("Пятёрочка"), "pyaterochka");
        assert_eq!(normalize_brand("Магнит"), "magnit");
        assert_eq!(normalize_brand("Красное и Белое"), "krasnoe_i_beloe");
        assert_eq!(normalize_brand("Burger King"), "burger_king");
        assert_eq!(normalize_brand("KFC"), "kfc");
        assert_eq!(normalize_brand("Вкусно — и точка"), "vkusno_i_tochka");
    }
}
