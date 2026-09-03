use std::path::Path;

use eccodes::{CodesFile, FallibleIterator, KeyWrite, ProductKind};
use log::debug;

/// converts the bytes from the grib1 file to a grib2 file by setting all
/// messages "edition" key to 2, then writing to a new file at the given `fp`
pub fn grib1_to_grib2(grib1_bytes: Vec<u8>, fp: &Path) -> anyhow::Result<()> {
    let bytes = grib1_bytes.len();
    let mut handle = CodesFile::new_from_memory(grib1_bytes, ProductKind::GRIB)?;
    let mut messages = 0usize;
    while let Some(msg) = handle.ref_message_iter().next()? {
        let mut msg = msg.try_clone()?;
        msg.write_key_unchecked("edition", 2)?;
        msg.write_to_file(fp, true)?;
        messages += 1;
    }
    debug!(
        "converted {messages} grib1 message(s) ({bytes} bytes) to grib2 at {}",
        fp.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use eccodes::{CodesFile, FallibleIterator, KeyRead, ProductKind};
    use std::path::Path;

    #[test]
    #[ignore = "local grib file needs to exist"]
    fn load_grib_file() -> Result<(), Box<dyn std::error::Error>> {
        let fname = Path::new("testdata/try_load_file_test.grib");
        let mut handle = CodesFile::new_from_file(fname, ProductKind::GRIB)?;

        let msg = handle
            .ref_message_iter()
            .next()?
            .ok_or("could not find first")?;

        let lats: Vec<f64> = msg.read_key("latitudes")?;
        let lons: Vec<f64> = msg.read_key("longitudes")?;
        let values: Vec<f64> = msg.read_key("values")?;

        for ((lat, lon), value) in lats.iter().zip(lons.iter()).zip(values.iter()) {
            println!("{lat} {lon} {value}");
        }

        Ok(())
    }
}
