# convection
main repo for the weather/heat model software

## Development

Use the `justfile` at the repository root for building, testing, checking the software etc. See [just](https://github.com/casey/just) for more info.

## Reference Material to implement
Collection of useful resources and libraries in order to implement:
- algorithms/mathematical tools to deduce weather patterns
- Mapping of geospatial data to view the weathe on top of
- The rendering part of the application
- The weather data retrieval and encoding
- and probably a lot more

To get weather data, [ECMWF](https://www.ecmwf.int/en/computing/software/ecmwf-web-api) seems promising.
For geological data, use open street map exported to geojson? For geojson et al. a good ref seems to be [this](https://georust.org/)

### Formats
- Weather
  - NetCDF > [rust binding](https://github.com/georust/netcdf/)
- Geo
  - geojson > [rust crate](https://github.com/georust/geozero)
  - general geo primitives > [rust crate](https://github.com/georust/geo)

### Computation
- PDE (partial differential equations)
  - either own impl or something like [russel_pde](https://github.com/cpmech/russell)
  - would greatly benefit from being in a compute shader in wgsl for high parallelism
- [Navier-Strokes](https://en.wikipedia.org/wiki/Navier%E2%80%93Stokes_equations)
  - describes the motion of viscous fluids, which can be applied to Weather
- Take [coriolis force](https://en.wikipedia.org/wiki/Coriolis_force) into context
