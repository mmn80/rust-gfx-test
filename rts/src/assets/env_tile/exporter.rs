use std::collections::HashMap;

use building_blocks::prelude::*;

use crate::{
    assets::env_tile::EnvTileAssetData,
    env::terrain::{Terrain, TerrainVoxel},
};

// don't know how to do it from distill
pub struct EnvTileExporter;

impl EnvTileExporter {
    pub fn export(name: String, voxels: Array3x1<TerrainVoxel>, terrain: &Terrain) -> Option<()> {
        let (min, shape) = (voxels.extent().minimum, voxels.extent().shape);
        let mut palette = vec![];
        let mut palette_builder = HashMap::new();
        let mut voxels_str = vec![];
        for z in min.z()..min.z() + shape.z() {
            let mut slice = vec![];
            for y in min.y()..min.y() + shape.y() {
                let mut line: String = "".to_owned();
                for x in min.x()..min.x() + shape.x() {
                    let voxel = voxels.get(PointN([x, y, z]));
                    let voxel_str = terrain.get_pallete_voxel_string(
                        &voxel,
                        &mut palette,
                        &mut palette_builder,
                    );
                    line.push_str(&voxel_str);
                }
                slice.push(line);
            }
            voxels_str.push(slice);
        }
        let asset_data = EnvTileAssetData {
            name: name.clone(),
            palette,
            voxels: voxels_str,
        };
        let asset_string =
            ron::ser::to_string_pretty::<EnvTileAssetData>(&asset_data, Default::default()).ok()?;
        std::fs::write(Self::get_tile_path(&name, true), asset_string).ok()
    }

    pub fn get_tile_path(tile_name: &str, include_root_dir: bool) -> String {
        let file_name = Self::get_tile_file_name(tile_name);
        if include_root_dir {
            format!("assets/tiles/{}", file_name)
        } else {
            format!("tiles/{}", file_name)
        }
    }

    pub fn get_tile_file_name(tile_name: &str) -> String {
        let file_name = tile_name.to_lowercase().replace(" ", "_");
        format!("{}.tile", file_name)
    }
}
