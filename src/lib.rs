#![warn(
    clippy::unwrap_used,
    clippy::cast_lossless,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::expect_used
)]

use geo::{Coord, MapCoords};

use geodesy::ctx::Context;

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
        })
    }

    pub fn from_geodesy(
        ctx: &'a C,
        source: geodesy::ctx::OpHandle,
        target: geodesy::ctx::OpHandle,
    ) -> Result<Self, Error> {
        Ok(Transformer {
            ctx,
            source,
            target,
        })
    }

    pub fn transform<Scalar: geo::CoordFloat>(
        &self,
        geometry: &mut geo::Geometry<Scalar>,
    ) -> Result<(), Error> {
        let mut transformed = geometry.try_map_coords::<Error>(|coord| {
            let mut coord = [geodesy::coord::Coor2D::gis(
                coord.x.to_f64().ok_or(Error::CouldNotConvertToF64)?,
                coord.y.to_f64().ok_or(Error::CouldNotConvertToF64)?,
            )];
            self.ctx
                .apply(self.source, geodesy::Direction::Inv, &mut coord)?;
            self.ctx
                .apply(self.target, geodesy::Direction::Fwd, &mut coord)?;
            // Geodesy outputs coordinates in radians, so we need to convert them back to degrees
            Ok(Coord {
                x: Scalar::from(coord[0].0[0].to_degrees()).ok_or(Error::CouldNotConvertFromF64)?,
                y: Scalar::from(coord[0].0[1].to_degrees()).ok_or(Error::CouldNotConvertFromF64)?,
            })
        })?;

        std::mem::swap(&mut transformed, geometry);

        Ok(())
    }
}

fn geodesy_ctx() -> geodesy::ctx::Minimal {
    geodesy::ctx::Minimal::new()
}
