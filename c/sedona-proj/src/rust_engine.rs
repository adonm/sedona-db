// Pure-Rust CrsEngine using proj-wkt/proj-core.
// No native libproj dependency when the `proj-rust` feature is enabled.

#[cfg(feature = "proj-rust")]
mod inner {
    use std::cell::RefCell;
    use std::rc::Rc;

    use datafusion_common::{DataFusionError, Result};
    use sedona_geometry::bounding_box::BoundingBox;
    use sedona_geometry::error::SedonaGeometryError;
    use sedona_geometry::transform::{CachingCrsEngine, CrsEngine, CrsTransform};

    fn crs_err(msg: String) -> SedonaGeometryError {
        SedonaGeometryError::External(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            msg,
        )))
    }

    #[derive(Debug)]
    pub struct RustCrsEngine;

    impl CrsEngine for RustCrsEngine {
        fn get_transform_crs_to_crs(
            &self,
            from: &str,
            to: &str,
            _aoi: Option<BoundingBox>,
            _opts: &str,
        ) -> std::result::Result<Rc<dyn CrsTransform>, SedonaGeometryError> {
            let t = proj_wkt::transform_from_crs_strings(from, to)
                .map_err(|e| crs_err(format!("proj-wkt ({from}→{to}): {e}")))?;
            Ok(Rc::new(RustCrsTransform(t)))
        }

        fn get_transform_pipeline(
            &self,
            _: &str,
            _: &str,
        ) -> std::result::Result<Rc<dyn CrsTransform>, SedonaGeometryError> {
            Err(crs_err("pipeline transforms not supported by Rust CRS engine".into()))
        }
    }

    pub struct RustCrsTransform(proj_core::transform::Transform);

    impl std::fmt::Debug for RustCrsTransform {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("RustCrsTransform").finish_non_exhaustive()
        }
    }

    impl CrsTransform for RustCrsTransform {
        fn transform_coord(
            &self,
            coord: &mut (f64, f64),
        ) -> std::result::Result<(), SedonaGeometryError> {
            let (x, y) = self
                .0
                .convert(*coord)
                .map_err(|e| crs_err(format!("proj-wkt convert: {e}")))?;
            coord.0 = x;
            coord.1 = y;
            Ok(())
        }
    }

    thread_local! {
        static RUST_ENGINE: RefCell<Option<CachingCrsEngine<RustCrsEngine>>> =
            const { RefCell::new(None) };
    }

    pub fn configure_global_rust_engine() -> Result<()> {
        RUST_ENGINE.with(|cell| {
            let mut guard = cell.borrow_mut();
            if guard.is_some() {
                Err(DataFusionError::Execution(
                    "Rust CRS engine already configured".into(),
                ))
            } else {
                *guard = Some(CachingCrsEngine::new(RustCrsEngine));
                Ok(())
            }
        })
    }

    pub fn with_global_engine<R>(
        f: impl FnOnce(&dyn CrsEngine) -> Result<R>,
    ) -> Result<R> {
        let mut f = Some(f);
        let mut result: Option<Result<R>> = None;
        RUST_ENGINE.with(|cell| {
            if let Some(engine) = cell.borrow().as_ref() {
                result = Some((f.take().unwrap())(engine));
            }
        });
        if let Some(r) = result {
            return r;
        }
        // Fall back to proj-sys engine
        crate::transform::with_global_proj_engine(|e| (f.take().unwrap())(e))
    }
}

#[cfg(not(feature = "proj-rust"))]
mod inner {
    use datafusion_common::Result;
    use sedona_geometry::transform::CrsEngine;

    pub fn configure_global_rust_engine() -> Result<()> {
        Err(datafusion_common::DataFusionError::NotImplemented(
            "proj-rust feature not enabled".into(),
        ))
    }

    pub fn with_global_engine<R>(
        f: impl FnOnce(&dyn CrsEngine) -> Result<R>,
    ) -> Result<R> {
        let mut f = Some(f);
        crate::transform::with_global_proj_engine(|e| (f.take().unwrap())(e))
    }
}

pub use inner::{configure_global_rust_engine, with_global_engine};
