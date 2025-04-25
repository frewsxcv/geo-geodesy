# geo-geodesy

A crate providing `geodesy` operations for the `geo` crate, enabling coordinate transformations between different Coordinate Reference Systems (CRS) using EPSG codes.

## Features

- Transform `geo::Geometry` coordinates between different EPSG defined Coordinate Reference Systems.
- Utilizes the `geodesy` and `crs-definitions` crates for transformation logic and CRS definitions.

## Usage

Here's a basic example of transforming a point from one CRS to another:

```rust
use geo::{point, Point};
use geo_geodesy::Transformer;

fn main() -> Result<(), geo_geodesy::Error> {
    // Example: Transform a point from WGS 84 (EPSG:4326) to UTM zone 32N (EPSG:32632)
    let source_epsg = 4326; // WGS 84
    let target_epsg = 32632; // UTM zone 32N

    let transformer = Transformer::setup(source_epsg, target_epsg)?;

    let mut point: Point<f64> = point!(x: 8.5, y: 47.3); // Zurich coordinates in WGS 84

    transformer.transform(&mut point.into())?; // Transform the point

    println!("Original point (EPSG:{}) : {:?}", source_epsg, point);
    println!("Transformed point (EPSG:{}) : {:?}", target_epsg, point);

    Ok(())
}
```

## License

This crate is licensed under either of

- Apache License, Version 2.0
- MIT license

at your option.
