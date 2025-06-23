use geonames::GeoPosition;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // There are files with a threshold of 500, 1000, 5000, and 15000.
    let threshold = 5000;
    let url = format!(
        "https://download.geonames.org/export/dump/cities{}.zip",
        threshold
    );
    println!("Downloading {}", url);
    let mut response = reqwest::get(url).await?;
    let length = response.content_length().unwrap_or(0) as usize;
    let mut data = Vec::with_capacity(length);
    while let Some(chunk) = response.chunk().await? {
        data.extend_from_slice(&chunk);
        print!("\rzip: {}/{}", data.len(), length);
    }
    println!();

    let mut zip = zip::ZipArchive::new(io::Cursor::new(data))?;
    let file = zip.by_name(&format!("cities{}.txt", threshold))?;
    let bufread = io::BufReader::new(file);
    let mut sorted_data = Vec::new();
    for line_res in bufread.lines() {
        let line = line_res?;
        let mut parts = line.split('\t');
        let Some(_id) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        let Some(_ascii_name) = parts.next() else {
            continue;
        };
        let Some(alternate_names) = parts.next() else {
            continue;
        };
        let Some(latitude) = parts.next() else {
            continue;
        };
        let Some(longitude) = parts.next() else {
            continue;
        };
        let Some(_feature_class) = parts.next() else {
            continue;
        };
        let Some(_feature_code) = parts.next() else {
            continue;
        };
        let Some(_country_code) = parts.next() else {
            continue;
        };
        let Some(_alternate_country_codes) = parts.next() else {
            continue;
        };
        let Some(_admin1_code) = parts.next() else {
            continue;
        };
        let Some(_admin2_code) = parts.next() else {
            continue;
        };
        let Some(_admin3_code) = parts.next() else {
            continue;
        };
        let Some(_admin4_code) = parts.next() else {
            continue;
        };
        let Some(population) = parts.next() else {
            continue;
        };
        let Some(_elevation) = parts.next() else {
            continue;
        };
        let Some(_digital_elevation_model) = parts.next() else {
            continue;
        };
        let Some(timezone) = parts.next() else {
            continue;
        };
        let Some(_modification_date) = parts.next() else {
            continue;
        };

        let geoposition = GeoPosition {
            latitude: latitude.parse()?,
            longitude: longitude.parse()?,
        };

        let timezone = timezone.to_string();
        sorted_data.push((population.parse::<u64>()?, timezone, geoposition));
    }

    sorted_data.sort_by(|a, b| b.0.cmp(&a.0));

    let mut timezone_positions = BTreeMap::new();
    for (_, timezone, geoposition) in sorted_data {
        if !timezone_positions.contains_key(&timezone) {
            timezone_positions.insert(timezone, geoposition);
        }
    }

    for (timezone, geoposition) in &timezone_positions {
        eprintln!("{timezone}: {geoposition:?}");
    }

    println!("timezone-geodata: {}", timezone_positions.len());

    let bitcode = bitcode::encode(&timezone_positions);
    println!("bitcode: {}", bitcode.len());
    fs::write("../data/timezone-geodata.bitcode-v0-6", bitcode)?;

    Ok(())
}
