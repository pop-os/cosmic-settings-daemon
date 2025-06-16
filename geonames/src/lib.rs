pub use bitcode;

#[derive(Clone, Debug, bitcode::Decode, bitcode::Encode)]
pub struct GeoPosition {
    pub latitude: f64,
    pub longitude: f64,
}
