pub mod client;
pub mod types;

#[macro_export]
/// creates a compile-time checked [`ERA5Year`] using const assertion
macro_rules! era5_year {
    ($year:expr) => {{
        const _: () = {
            const YEAR: u16 = $year;
            assert!(YEAR >= 1940, "Year must be >= 1940");
        };
        $crate::ecmwf_client::types::ERA5Year::new($year).unwrap()
    }};
}
