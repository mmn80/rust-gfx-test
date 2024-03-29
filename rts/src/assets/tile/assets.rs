use std::sync::Arc;

use building_blocks::prelude::*;
use rafx::{
    api::{RafxError, RafxResult},
    assets::{AssetManager, DefaultAssetTypeHandler, DefaultAssetTypeLoadHandler},
};
use serde::{Deserialize, Serialize};
use type_uuid::*;

use crate::env::simulation::MaterialVoxel;

#[derive(TypeUuid, Serialize, Deserialize, Debug, Clone)]
#[uuid = "e0b18b31-dcff-4e31-85dd-2e224bb1d04b"]
pub struct TileAssetData {
    pub name: String,
    pub palette: Vec<String>,
    pub voxels: Vec<Vec<String>>,
}

pub struct TileAssetInner {
    pub name: String,
    pub palette: Vec<String>,
    pub voxels: Array3x1<MaterialVoxel>,
}

#[derive(TypeUuid, Clone)]
#[uuid = "76097c2c-4d34-4957-bae1-8369f4a1d856"]
pub struct TileAsset {
    pub inner: Arc<TileAssetInner>,
}

pub struct TileLoadHandler;

impl DefaultAssetTypeLoadHandler<TileAssetData, TileAsset> for TileLoadHandler {
    #[profiling::function]
    fn load(_asset_manager: &mut AssetManager, asset_data: TileAssetData) -> RafxResult<TileAsset> {
        if asset_data.palette.len() > 256 {
            return Err(RafxError::StringError(format!(
                "Pallete has {} materials but only 256 are supported.",
                asset_data.palette.len()
            )));
        }

        let x_max = asset_data
            .voxels
            .iter()
            .map(|slice| {
                slice
                    .iter()
                    .map(|line| line.len() / 2)
                    .max()
                    .unwrap_or_default()
            })
            .max()
            .unwrap_or_default() as i32;
        let y_max = asset_data
            .voxels
            .iter()
            .map(|slice| slice.len())
            .max()
            .unwrap_or_default() as i32;
        let z_max = asset_data.voxels.len() as i32;

        let mut voxels = Array3x1::<MaterialVoxel>::fill(
            Extent3i::from_min_and_shape(Point3i::ZERO, PointN([x_max, y_max, z_max])),
            MaterialVoxel::empty(),
        );

        for (z, slice) in asset_data.voxels.iter().enumerate() {
            let z = z as i32;
            for (y, line) in slice.iter().enumerate() {
                let y = y as i32;
                if line.len() % 2 != 0 {
                    return Err(RafxError::StringError(format!(
                        "Invalid voxel line '{}'. String of hex pairs expected.",
                        line
                    )));
                }
                let mut x = 0;
                while 2 * x < line.len() {
                    let mat_str = &line[2 * x..2 * x + 2];
                    if let Ok(mat) = u16::from_str_radix(mat_str, 16) {
                        if mat > asset_data.palette.len() as u16 {
                            return Err(RafxError::StringError(format!(
                                "Invalid material index '{}'. Pallete has {} materials.",
                                mat,
                                asset_data.palette.len()
                            )));
                        }
                        *voxels.get_mut(PointN([x as i32, y, z])) =
                            MaterialVoxel::from_material_index(mat);
                    } else {
                        return Err(RafxError::StringError(format!(
                            "Invalid voxel characters '{}'. Hex string expected.",
                            mat_str
                        )));
                    }
                    x += 1;
                }
            }
        }

        Ok(TileAsset {
            inner: Arc::new(TileAssetInner {
                name: asset_data.name,
                palette: asset_data.palette,
                voxels,
            }),
        })
    }
}

pub type TileAssetType = DefaultAssetTypeHandler<TileAssetData, TileAsset, TileLoadHandler>;
