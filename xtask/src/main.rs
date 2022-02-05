fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let _ = args.next();
    let task_name = args.next();
    match task_name.as_deref() {
        Some("update-products") => Ok(xtask::update_products()?),
        _ => Ok(()),
    }
}
