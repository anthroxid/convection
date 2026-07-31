# copernicus-core

wrapper and glue code between netcdf, grib - turning it into data with a common denominator.
also has a feature for retrieving data using the copernicus CDS API (see link below).

## Useful resources/links
- get data at [copernicus](https://cds.climate.copernicus.eu/) or [ecmwf public ftp server](https://data.ecmwf.int/forecasts/20260704/)
- read up on netCDF file format and it's use in scientific data analysis and weather reports [here](https://climateestimate.net/content/netcdfs-and-basic-coding.html)

## Getting data
To easily get data, use the climate data store API from copernicus.eu.


## Converting grib to grib2
- you can either use:
```bash
grib_set -s edition=2 testdata/try_load_file_test.grib testdata/try_load_file_test.grib2
```
for [this command](https://confluence.ecmwf.int/spaces/ECC/pages/45747233/grib_set), you will need a package called `eccodes`, which is developed by ECMWF, there also seems to be a rust library for it.

- or the rust function `grib1_to_grib2`, which will take a grib1 file buffer and write it sequentially to set its edition to 2.

conversion to grib2 is normally only necessary when wanting to use the `grib` crate in rust, which only supports parsing the grib2 file format (recheck correctness).

## Special Tests

To test the file retrieval, you can run the ignored test `try_download_file`. It is ignored because it requires setting an environment variable for the CDS API.
You may run the test like so:
```bash
LOCAL_DEBUG_COPERNICUS_API_KEY="insert-your-api-key" \
  cargo test try_download_file --all-features -- --nocapture --include-ignored
```

To test the GRIB file parsing, you can run the test above ^^
After doing this, you can run the file loading test like so:
```bash
cargo test load_grib_file -- --nocapture --include-ignored
```
