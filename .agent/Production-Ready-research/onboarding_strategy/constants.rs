//! ═══════════════════════════════════════════════════════════════════════
//!  Business Profile Constants — Ported from Web Frontend constants.ts
//! ═══════════════════════════════════════════════════════════════════════
//!  These constants are the SINGLE SOURCE OF TRUTH for:
//!  - Account types (Personal / Business)
//!  - Industry taxonomy (15 industries + sub-categories)
//!  - Business models
//!  - Target audiences
//!  - Primary goals
//!
//!  Reusable across CLI, Desktop App, Website — all share these constants.
//!  User selects during onboarding, can change later in Settings.
//! ═══════════════════════════════════════════════════════════════════════

use serde::Serialize;

// ── Generic Option Item ───────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
pub struct OptionItem {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct SubCategory {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct IndustryEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub sub_categories: &'static [SubCategory],
}

// ── Account Types ─────────────────────────────────────────────────────

pub const ACCOUNT_TYPES: &[OptionItem] = &[
    OptionItem { id: "personal", label: "Individual / Personal Assistant", icon: "👤" },
    OptionItem { id: "business", label: "Business Owner", icon: "🏢" },
];

// ── Primary Goals ─────────────────────────────────────────────────────

pub const PRIMARY_GOALS: &[OptionItem] = &[
    OptionItem { id: "increase_sales", label: "Increase Sales / Revenue", icon: "💰" },
    OptionItem { id: "improve_support", label: "Improve Customer Support", icon: "🤝" },
    OptionItem { id: "automate_booking", label: "Automate Appointment Booking", icon: "📅" },
    OptionItem { id: "generate_leads", label: "Generate More Leads", icon: "📢" },
    OptionItem { id: "reduce_costs", label: "Reduce Operational Costs", icon: "🔄" },
    OptionItem { id: "team_productivity", label: "Improve Team Productivity", icon: "👥" },
    OptionItem { id: "insights", label: "Better Customer Insights", icon: "📊" },
    OptionItem { id: "other", label: "Other", icon: "🔧" },
];

// ── Business Models ───────────────────────────────────────────────────

pub const BUSINESS_MODELS: &[OptionItem] = &[
    OptionItem { id: "product", label: "Product-based", icon: "🛒" },
    OptionItem { id: "service", label: "Service-based", icon: "🤝" },
    OptionItem { id: "subscription", label: "Subscription", icon: "📦" },
    OptionItem { id: "marketplace", label: "Marketplace / Platform", icon: "🎫" },
    OptionItem { id: "hybrid", label: "Hybrid", icon: "💡" },
    OptionItem { id: "other", label: "Other", icon: "🔧" },
];

// ── Target Audiences ──────────────────────────────────────────────────

pub const AUDIENCES: &[OptionItem] = &[
    OptionItem { id: "b2b", label: "B2B (Businesses)", icon: "🏢" },
    OptionItem { id: "b2c", label: "B2C (Consumers)", icon: "👤" },
    OptionItem { id: "b2g", label: "B2G (Government)", icon: "🏫" },
    OptionItem { id: "all", label: "All of the above", icon: "🔀" },
    OptionItem { id: "other", label: "Other", icon: "🔧" },
];

// ── Industry Taxonomy ─────────────────────────────────────────────────
// 15 industries, each with sub-categories.
// Mirrors the web frontend's INDUSTRY_TAXONOMY from constants.ts

pub const INDUSTRY_TAXONOMY: &[IndustryEntry] = &[
    IndustryEntry {
        id: "ecommerce",
        label: "E-commerce / Retail",
        icon: "🛍️",
        sub_categories: &[
            SubCategory { id: "d2c", label: "Direct-to-Consumer (D2C Brand)", icon: "🏷️" },
            SubCategory { id: "marketplace", label: "Amazon/Flipkart Seller", icon: "📦" },
            SubCategory { id: "dropshipping", label: "Dropshipping", icon: "🚚" },
            SubCategory { id: "local_retail", label: "Local Physical Store", icon: "🏪" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "saas",
        label: "Software & IT",
        icon: "💻",
        sub_categories: &[
            SubCategory { id: "b2b_saas", label: "B2B SaaS Platform", icon: "🏢" },
            SubCategory { id: "b2c_saas", label: "B2C Web App", icon: "🌐" },
            SubCategory { id: "app_dev", label: "Mobile App Development", icon: "📱" },
            SubCategory { id: "it_services", label: "IT Support Services", icon: "🛠️" },
            SubCategory { id: "enterprise_sw", label: "Enterprise Software", icon: "🖥️" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "real_estate",
        label: "Real Estate & Property",
        icon: "🏠",
        sub_categories: &[
            SubCategory { id: "brokerage", label: "Real Estate Brokerage", icon: "🏠" },
            SubCategory { id: "property_mgmt", label: "Property Management", icon: "🔑" },
            SubCategory { id: "construction", label: "Construction & Builders", icon: "🏗️" },
            SubCategory { id: "coworking", label: "Co-working Spaces", icon: "🪑" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "healthcare",
        label: "Healthcare & Wellness",
        icon: "🏥",
        sub_categories: &[
            SubCategory { id: "clinic", label: "Clinic / Hospital", icon: "🏥" },
            SubCategory { id: "telehealth", label: "Telehealth / Digital Health", icon: "💊" },
            SubCategory { id: "pharmacy", label: "Pharmacy", icon: "💉" },
            SubCategory { id: "mental_health", label: "Mental Health Services", icon: "🧠" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "education",
        label: "Education & EdTech",
        icon: "🎓",
        sub_categories: &[
            SubCategory { id: "k12", label: "K-12 School / Institute", icon: "🏫" },
            SubCategory { id: "edtech", label: "EdTech Platform", icon: "💡" },
            SubCategory { id: "tutoring", label: "Tutoring / Coaching Center", icon: "📚" },
            SubCategory { id: "online_courses", label: "Online Course Creator", icon: "🎓" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "agencies",
        label: "Creative & Marketing Agencies",
        icon: "🎨",
        sub_categories: &[
            SubCategory { id: "digital_marketing", label: "Digital Marketing / SEO", icon: "📈" },
            SubCategory { id: "design", label: "Design & UX/UI Agency", icon: "🎨" },
            SubCategory { id: "pr", label: "Public Relations (PR)", icon: "📣" },
            SubCategory { id: "video_prod", label: "Video Production", icon: "🎬" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "food",
        label: "Food & Beverage",
        icon: "🍔",
        sub_categories: &[
            SubCategory { id: "restaurant", label: "Restaurant / Cafe", icon: "🍽️" },
            SubCategory { id: "cloud_kitchen", label: "Cloud Kitchen", icon: "👨‍🍳" },
            SubCategory { id: "fmcg", label: "FMCG Brand", icon: "🛒" },
            SubCategory { id: "catering", label: "Catering Services", icon: "🍱" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "finance",
        label: "Finance & Insurance",
        icon: "💰",
        sub_categories: &[
            SubCategory { id: "fintech", label: "Fintech Startup", icon: "💳" },
            SubCategory { id: "accounting", label: "Accounting / CA Firm", icon: "📊" },
            SubCategory { id: "insurance", label: "Insurance Broker", icon: "🛡️" },
            SubCategory { id: "wealth_mgmt", label: "Wealth Management", icon: "💹" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "fitness",
        label: "Gym & Fitness",
        icon: "🏋️",
        sub_categories: &[
            SubCategory { id: "gym", label: "Gym / Fitness Studio", icon: "🏋️" },
            SubCategory { id: "personal_trainer", label: "Personal Trainer", icon: "🤸" },
            SubCategory { id: "fitness_app", label: "Fitness App / Platform", icon: "📲" },
            SubCategory { id: "sports", label: "Sports Academy", icon: "⚽" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "manufacturing",
        label: "Manufacturing & Logistics",
        icon: "🏭",
        sub_categories: &[
            SubCategory { id: "factory", label: "Factory / Production", icon: "🏭" },
            SubCategory { id: "supply_chain", label: "Logistics & Supply Chain", icon: "🚛" },
            SubCategory { id: "wholesale", label: "Wholesale Distribution", icon: "📦" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "hospitality",
        label: "Hospitality & Travel",
        icon: "🏨",
        sub_categories: &[
            SubCategory { id: "hotel", label: "Hotel / Resort", icon: "🏨" },
            SubCategory { id: "travel_agency", label: "Travel Agency", icon: "✈️" },
            SubCategory { id: "events", label: "Event Management", icon: "🎉" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "consulting",
        label: "Professional Consulting",
        icon: "💼",
        sub_categories: &[
            SubCategory { id: "management", label: "Management Consulting", icon: "📋" },
            SubCategory { id: "hr", label: "HR & Recruitment", icon: "👥" },
            SubCategory { id: "legal", label: "Legal Services / Law Firm", icon: "⚖️" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "creators",
        label: "Creators & Influencers",
        icon: "📸",
        sub_categories: &[
            SubCategory { id: "content_creator", label: "YouTuber / Content Creator", icon: "🎥" },
            SubCategory { id: "community", label: "Community Builder", icon: "🌐" },
            SubCategory { id: "podcaster", label: "Podcaster", icon: "🎙️" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "automotive",
        label: "Automotive",
        icon: "🚗",
        sub_categories: &[
            SubCategory { id: "dealership", label: "Car Dealership", icon: "🚗" },
            SubCategory { id: "repair", label: "Auto Repair / Service", icon: "🔧" },
            SubCategory { id: "rentals", label: "Car Rentals", icon: "🚕" },
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
    IndustryEntry {
        id: "other",
        label: "Other",
        icon: "🔧",
        sub_categories: &[
            SubCategory { id: "other", label: "Other", icon: "🔧" },
        ],
    },
];

// ── Helper Functions ──────────────────────────────────────────────────

/// Find industry entry by id
pub fn find_industry(id: &str) -> Option<&'static IndustryEntry> {
    INDUSTRY_TAXONOMY.iter().find(|i| i.id == id)
}

/// Get sub-categories for a given industry id
pub fn get_sub_categories(industry_id: &str) -> &'static [SubCategory] {
    find_industry(industry_id)
        .map(|i| i.sub_categories)
        .unwrap_or(&[])
}
