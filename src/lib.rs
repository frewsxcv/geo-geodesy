#![warn(
    clippy::unwrap_used,
    clippy::cast_lossless,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::expect_used
)]

use geo::{Coord, MapCoords};

use geodesy::{coord::CoordinateTuple, ctx::Context};

#[derive(Debug)]
pub enum Error {
    Geodesy(geodesy::Error),
    UnknownEpsgCode(u16),
    CouldNotConvertToF64,
    CouldNotConvertFromF64,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Geodesy(err) => write!(f, "Geodesy error: {}", err),
            Error::UnknownEpsgCode(code) => write!(f, "Unknown EPSG code: {}", code),
            Error::CouldNotConvertToF64 => write!(f, "Could not convert number to f64"),
            Error::CouldNotConvertFromF64 => write!(f, "Could not convert number from f64"),
        }
    }
}

impl std::error::Error for Error {}

impl From<geodesy::Error> for Error {
    fn from(err: geodesy::Error) -> Self {
        Error::Geodesy(err)
    }
}

pub struct Transformer<'a, C: geodesy::ctx::Context> {
    ctx: &'a C,
    source: geodesy::ctx::OpHandle,
    target: geodesy::ctx::OpHandle,
    /// Whether a CRS is geographic (lon/lat). When true, geodesy input degrees
    /// are converted to radians, and geodesy output radians are converted to degrees.
    /// When false (projected CRS like EPSG:3857), the input and/or output is in the
    /// projection's linear unit (e.g. metres) and must not be converted.
    source_is_geographic: bool,
    target_is_geographic: bool,
}

fn is_geographic_proj4(proj4: &str) -> bool {
    proj4.contains("+proj=longlat")
}

impl<'a, C: geodesy::ctx::Context> Transformer<'a, C> {
    pub fn from_epsg(ctx: &'a mut C, source_crs: u16, target_crs: u16) -> Result<Self, Error> {
        let source =
            crs_definitions::from_code(source_crs).ok_or(Error::UnknownEpsgCode(source_crs))?;
        let target =
            crs_definitions::from_code(target_crs).ok_or(Error::UnknownEpsgCode(target_crs))?;
        let source_geodesy_string = geodesy::authoring::parse_proj(source.proj4)?;
        let source_op_handle = ctx.op(&source_geodesy_string)?;
        let target_geodesy_string = geodesy::authoring::parse_proj(target.proj4)?;
        let target_op_handle = ctx.op(&target_geodesy_string)?;
        Ok(Transformer {
            ctx,
            source: source_op_handle,
            target: target_op_handle,
            source_is_geographic: is_geographic_proj4(source.proj4),
            target_is_geographic: is_geographic_proj4(target.proj4),
        })
    }

    pub fn from_geodesy(
        ctx: &'a C,
        source: geodesy::ctx::OpHandle,
        target: geodesy::ctx::OpHandle,
        source_is_geographic: bool,
        target_is_geographic: bool,
    ) -> Result<Self, Error> {
        Ok(Transformer {
            ctx,
            source,
            target,
            source_is_geographic,
            target_is_geographic,
        })
    }

    pub fn transform<Scalar: geo::CoordFloat>(
        &self,
        geometry: &mut geo::Geometry<Scalar>,
    ) -> Result<(), Error> {
        let target_is_geographic = self.target_is_geographic;
        let mut transformed = geometry.try_map_coords::<Error>(|coord| {
            let in_x = coord.x.to_f64().ok_or(Error::CouldNotConvertToF64)?;
            let in_y = coord.y.to_f64().ok_or(Error::CouldNotConvertToF64)?;

            let mut coord = if self.source_is_geographic {
                // Geographic CRS: geodesy expects radians, convert from degrees
                [geodesy::coord::Coor2D::gis(in_x, in_y)]
            } else {
                // Projected CRS: geodesy expects linear units (e.g. metres)
                [geodesy::coord::Coor2D::raw(in_x, in_y)]
            };

            self.ctx
                .apply(self.source, geodesy::Direction::Inv, &mut coord)?;
            self.ctx
                .apply(self.target, geodesy::Direction::Fwd, &mut coord)?;
            let (x, y) = if target_is_geographic {
                // Geographic CRS: geodesy outputs radians, convert to degrees
                coord[0].xy_to_degrees()
            } else {
                // Projected CRS: geodesy outputs linear units (e.g. metres)
                coord[0].xy()
            };
            Ok(Coord {
                x: Scalar::from(x).ok_or(Error::CouldNotConvertFromF64)?,
                y: Scalar::from(y).ok_or(Error::CouldNotConvertFromF64)?,
            })
        })?;

        std::mem::swap(&mut transformed, geometry);

        Ok(())
    }
}

fn geodesy_ctx() -> geodesy::ctx::Minimal {
    geodesy::ctx::Minimal::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::{Geometry, Point};

    /// Regression test: transforming from a geographic CRS (EPSG:4326) to a
    /// projected CRS (EPSG:3857 / Web Mercator) must preserve metre values.
    ///
    /// Before the fix, `Transformer::transform()` unconditionally called
    /// `.to_degrees()` on the output coordinates, which multiplied the metre
    /// values by 180/pi (~57.3), corrupting them.
    #[test]
    fn projected_crs_output_not_corrupted_by_to_degrees() {
        let mut ctx = geodesy_ctx();
        // EPSG:4326 = WGS 84 geographic (degrees)
        // EPSG:3857 = Web Mercator (metres)
        let transformer =
            Transformer::from_epsg(&mut ctx, 4326, 3857).expect("failed to create transformer");

        // London: approximately 51.5074 N, -0.1278 W
        let mut geometry: Geometry<f64> = Point::new(-0.1278, 51.5074).into();
        transformer
            .transform(&mut geometry)
            .expect("transform failed");

        let point = match &geometry {
            Geometry::Point(p) => p,
            other => panic!("expected Point, got {:?}", other),
        };

        // Web Mercator coordinates for London are roughly:
        //   x ≈ -14,226 m, y ≈ 6,711,344 m
        // The key invariant: if to_degrees() were wrongly applied, x and y
        // would be ~57.3x too large. We check that the values are in the
        // expected ballpark (within 1% of reference values).
        let expected_x: f64 = -14_226.0;
        let expected_y: f64 = 6_711_344.0;

        let x_err = (point.x() - expected_x).abs() / expected_x.abs();
        let y_err = (point.y() - expected_y).abs() / expected_y.abs();

        assert!(
            x_err < 0.01,
            "x coordinate {:.1} deviates from expected {:.1} by {:.1}% — \
             likely corrupted by to_degrees()",
            point.x(),
            expected_x,
            x_err * 100.0,
        );
        assert!(
            y_err < 0.01,
            "y coordinate {:.1} deviates from expected {:.1} by {:.1}% — \
             likely corrupted by to_degrees()",
            point.y(),
            expected_y,
            y_err * 100.0,
        );
    }

    #[test]
    fn degrees_to_radians() {
        let mut ctx = geodesy_ctx();

        let transformer =
            Transformer::from_epsg(&mut ctx, 3857, 4326).expect("failed to create transformer");

        // London: in Mercator at -14_226.0, 6_711_344.0, should be approximately 51.5074 N, -0.1278 W in WGS 84 geographic
        let mut geometry: Geometry<f64> = Point::new(-14_226.0, 6_711_344.0).into();
        transformer
            .transform(&mut geometry)
            .expect("transform failed");

        let point = match &geometry {
            Geometry::Point(p) => p,
            other => panic!("expected Point, got {:?}", other),
        };

        let expected_x: f64 = -0.1278;
        let expected_y: f64 = 51.5074;

        let x_err = (point.x() - expected_x).abs() / expected_x.abs();
        let y_err = (point.y() - expected_y).abs() / expected_y.abs();

        assert!(x_err < 0.01, "x coordinate {:.6} deviates from expected {:.6} by {:.1}% — likely corrupted by to_degrees()", point.x(), expected_x, x_err * 100.0);
        assert!(y_err < 0.01, "y coordinate {:.6} deviates from expected {:.6} by {:.1}% — likely corrupted by to_degrees()", point.y(), expected_y, y_err * 100.0);
    }
}
