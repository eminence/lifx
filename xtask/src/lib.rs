use std::{borrow::Cow, fs::File};

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct LifxProducts {
    pub vid: i32,
    pub name: String,
    pub products: Vec<LifxProduct>,
    pub defaults: LifxFeatures,
}

#[derive(Deserialize, Debug)]
pub struct LifxFeatures {
    #[serde(default)]
    hev: bool,
    #[serde(default)]
    color: bool,
    #[serde(default)]
    chain: bool,
    #[serde(default)]
    matrix: bool,
    #[serde(default)]
    relays: bool,
    #[serde(default)]
    buttons: bool,
    #[serde(default)]
    infrared: bool,
    #[serde(default)]
    multizone: bool,
    #[serde(default)]
    temperature_range: Option<Vec<u16>>,
}

#[derive(Deserialize, Debug)]
pub struct LifxProduct {
    pub pid: i32,
    pub name: String,
    pub features: LifxFeatures,
}

#[derive(Debug, Clone)]
enum TemperatureRange {
    /// The device supports a range of temperatures
    Variable { min: u16, max: u16 },
    /// The device only supports 1 temperature
    Fixed(u16),
    /// For devices that aren't lighting products (the LIFX switch)
    None,
}

impl From<Option<&[u16]>> for TemperatureRange {
    fn from(v: Option<&[u16]>) -> Self {
        match v {
            Some(&[min, max]) => TemperatureRange::Variable { min, max },
            Some(&[a]) => TemperatureRange::Fixed(a),
            None => TemperatureRange::None,
            x => panic!("Unexpected temperature range: {:?}", x),
        }
    }
}

impl TemperatureRange {
    fn fmt(&self) -> Cow<str> {
        match self {
            TemperatureRange::Variable { min, max } => Cow::from(format!(
                "TemperatureRange::Variable {{ min: {}, max: {} }} ",
                min, max
            )),
            TemperatureRange::Fixed(x) => Cow::from(format!("TemperatureRange::Fixed({})", x)),
            TemperatureRange::None => Cow::from("TemperatureRange::None"),
        }
    }
}

pub fn update_products() -> anyhow::Result<()> {
    let file = File::open("products.json")?;
    let products: Vec<LifxProducts> = serde_json::from_reader(file)?;
    assert_eq!(products.len(), 1);

    // We want to produce a string like the following, which we can copy/paste into lifx-core/src/lib.rs
    // (1, 1) => Some(&ProductInfo { name: "Original 1000", color: true, infrared: false, multizone: false, chain: false}),

    for prd in &products[0].products {
        let t = TemperatureRange::from(prd.features.temperature_range.as_deref());
        println!(
            r#"(1, {pid}) => Some(&ProductInfo {{ name: "{name}", color: {color}, infrared: {ir}, multizone: {mz}, chain: {chain}, hev: {hev}, matrix: {matrix}, relays: {relay}, buttons: {buttons}, temperature_range: {temp} }}),"#,
            pid = prd.pid,
            name = prd.name,
            color = prd.features.color,
            ir = prd.features.infrared,
            mz = prd.features.multizone,
            chain = prd.features.chain,
            hev = prd.features.hev,
            matrix = prd.features.matrix,
            relay = prd.features.relays,
            buttons = prd.features.buttons,
            temp = t.fmt()
        );
    }
    Ok(())
}
