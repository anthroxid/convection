use copernicus_core::{
    CdseClient, SentinelHubClient, cdse_client::types::StacSearchQuery,
    sentinel_client::types::ProcessRequest,
};

fn main() -> anyhow::Result<()> {
    let cdse = CdseClient::new(
        std::env::var("CDSE_CLIENT_ID")?,
        std::env::var("CDSE_CLIENT_SECRET")?,
    )?;
    let sentinelhub = SentinelHubClient::from_engine(cdse.engine().clone());

    // metadata fetch
    let results = cdse.search_stac(
        &StacSearchQuery::new()
            .collection("sentinel-2-l2a")
            .bbox([16.3, 48.1, 16.5, 48.3]) // Vienna
            .datetime_range("2026-06-01T00:00:00Z", "2026-07-20T00:00:00Z")
            .max_cloud_cover(20.0)
            .limit(5),
    )?;
    for item in &results.features {
        println!(
            "{} — {:?}, {:?}% cloud",
            item.id,
            item.datetime(),
            item.cloud_cover()
        );
    }

    // fetch and save file
    let png = sentinelhub.process_image(&ProcessRequest::true_color_s2(
        [16.3, 48.1, 16.5, 48.3],
        "2026-06-01T00:00:00Z",
        "2026-06-10T00:00:00Z",
        1024,
        1024,
    ))?;
    std::fs::write("/tmp/vienna_true_color.png", png)?;

    Ok(())
}
