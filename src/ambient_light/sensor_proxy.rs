#[zbus::proxy(
    default_service = "net.hadess.SensorProxy",
    interface = "net.hadess.SensorProxy",
    default_path = "/net/hadess/SensorProxy"
)]
pub trait SensorProxy {
    fn claim_light(&self) -> zbus::Result<()>;

    fn release_light(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn has_ambient_light(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn light_level(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn light_level_unit(&self) -> zbus::Result<String>;
}
