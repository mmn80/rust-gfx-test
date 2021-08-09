use glam::Vec3;
use legion::{IntoQuery, Read, Resources, World};
use rafx_plugins::{
    components::{
        DirectionalLightComponent, PointLightComponent, SpotLightComponent, TransformComponent,
    },
    features::debug3d::Debug3DResource,
};
use rand::Rng;

mod menu_scene;
use menu_scene::MenuScene;
mod main_scene;
use main_scene::MainScene;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Scene {
    Menu,
    Main,
}

fn random_color(rng: &mut impl Rng) -> Vec3 {
    let r = rng.gen_range(0.2..1.0);
    let g = rng.gen_range(0.2..1.0);
    let b = rng.gen_range(0.2..1.0);
    let v = Vec3::new(r, g, b);
    v.normalize()
}

fn create_scene(scene: Scene, world: &mut World, resources: &Resources) -> Box<dyn GameScene> {
    match scene {
        Scene::Menu => Box::new(MenuScene::new(world, resources)),
        Scene::Main => Box::new(MainScene::new(world, resources)),
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SceneManagerAction {
    None,
    Scene(Scene),
    Exit,
}

pub trait GameScene {
    fn update(&mut self, world: &mut World, resources: &mut Resources) -> SceneManagerAction;

    fn cleanup(&mut self, _world: &mut World, _resources: &Resources) {}
}

pub struct SceneManager {
    scene: Option<Box<dyn GameScene>>,
    current_scene: Scene,
    pub scene_action: SceneManagerAction,
}

impl Default for SceneManager {
    fn default() -> Self {
        SceneManager {
            scene: None,
            current_scene: Scene::Menu,
            scene_action: SceneManagerAction::Scene(Scene::Menu),
        }
    }
}

impl SceneManager {
    pub fn try_load_scene(&mut self, world: &mut World, resources: &Resources, next_scene: Scene) {
        if let Some(scene) = &mut self.scene {
            scene.cleanup(world, resources);
        }
        world.clear();
        log::info!("Load scene {:?}", next_scene);
        self.scene = Some(create_scene(next_scene, world, resources));
    }

    pub fn update_scene(
        &mut self,
        world: &mut World,
        resources: &mut Resources,
    ) -> SceneManagerAction {
        if let Some(scene) = &mut self.scene {
            scene.update(world, resources)
        } else {
            SceneManagerAction::None
        }
    }

    pub fn try_cleanup_current_scene(&mut self, world: &mut World, resources: &Resources) {
        if let Some(scene) = &mut self.scene {
            scene.cleanup(world, resources);
        }

        world.clear();
    }
}

fn add_light_debug_draw(resources: &Resources, world: &World) {
    let mut debug_draw = resources.get_mut::<Debug3DResource>().unwrap();

    let mut query = <Read<DirectionalLightComponent>>::query();
    for light in query.iter(world) {
        let light_from = light.direction * -10.0;
        let light_to = Vec3::ZERO;

        debug_draw.add_line(light_from, light_to, light.color);
    }

    let mut query = <(Read<TransformComponent>, Read<PointLightComponent>)>::query();
    for (transform, light) in query.iter(world) {
        debug_draw.add_sphere(transform.translation, 0.1, light.color, 12);
        debug_draw.add_sphere(transform.translation, light.range, light.color, 12);
    }

    let mut query = <(Read<TransformComponent>, Read<SpotLightComponent>)>::query();
    for (transform, light) in query.iter(world) {
        let light_from = transform.translation;
        let light_to = transform.translation + light.direction;
        let light_direction = (light_to - light_from).normalize();

        debug_draw.add_cone(
            light_from,
            light_from + (light.range * light_direction),
            light.range * light.spotlight_half_angle.tan(),
            light.color,
            10,
        );
    }
}

fn add_directional_light(
    _resources: &Resources,
    world: &mut World,
    light_component: DirectionalLightComponent,
) {
    world.extend(vec![(light_component,)]);
}

fn add_spot_light(
    _resources: &Resources,
    world: &mut World,
    position: Vec3,
    light_component: SpotLightComponent,
) {
    let position_component = TransformComponent {
        translation: position,
        ..Default::default()
    };

    world.extend(vec![(position_component, light_component)]);
}

fn add_point_light(
    _resources: &Resources,
    world: &mut World,
    position: Vec3,
    light_component: PointLightComponent,
) {
    let position_component = TransformComponent {
        translation: position,
        ..Default::default()
    };

    world.extend(vec![(position_component, light_component)]);
}
