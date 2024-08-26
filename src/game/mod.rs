use bevy::math::NormedVectorSpace;
use bevy::prelude::*;
use rand::prelude::*;
use rand_distr::*;

pub(super) fn plugin(game: &mut App) {
    game.init_resource::<ScreenHeight>()
        .init_resource::<LastPlatformSpawnHeight>()
        .add_systems(Startup, Player::spawn)
        .add_systems(
            FixedUpdate,
            (
                (
                    (player_horizontal_control, step_physics).chain(),
                    step_interpolation,
                ),
                (keep_player_in_bounds, screen_tracking),
                (
                    (platform_spawner, player_falling_jumping).chain(),
                    kill_player_on_damage,
                ),
            )
                .chain(),
        );
}

#[derive(Component, Debug, Deref, DerefMut)] // consider PartialEq
pub struct Velocity(pub Vec2);
impl Velocity {
    pub const JUMP_VELOCITY: f32 = 40.0;
    pub const GRAVITY: f32 = 10.0;
    pub const MAX_FALL_SPEED: f32 = 60.0;
    pub const HORIZONTAL_ACCELERATION: f32 = 125.0;
    pub const MAX_HORIZONTAL_SPEED: f32 = 30.0;
}

#[derive(Component, Debug)]
pub struct Line(pub Vec2, pub Vec2);

#[derive(Component, Debug, Deref)]
pub struct InterpolationTimer(pub Timer);

#[derive(Component, Debug, Deref)]
pub struct CollisionBox(pub Box);

#[derive(Debug)]
pub struct Box {
    pub width: f32,
    pub height: f32,
}
impl From<Vec2> for Box {
    fn from(value: Vec2) -> Self {
        Self {
            width: value.x,
            height: value.y,
        }
    }
}
impl Box {
    fn test_overlap(&self, self_pos: Vec2, other: &Self, other_pos: Vec2) -> bool {
        let combined_width = self.width + other.width;
        let combined_height = self.height + other.height;
        let x_distance = self_pos.x.distance(other_pos.x);
        let y_distance = self_pos.y.distance(other_pos.y);
        x_distance <= combined_width || y_distance <= combined_height
    }
}

fn player_horizontal_control(
    time: Res<Time>,
    mut player_query: Query<&mut Velocity, With<Player>>,
    kb: Res<ButtonInput<KeyCode>>,
) {
    let mut player_velocity = player_query.get_single_mut().unwrap();
    let left_press = kb.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]);
    let right_press = kb.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]);
    match (left_press, right_press) {
        (true, true) | (false, false) => (),
        (true, false) => {
            player_velocity.x = f32::max(
                -Velocity::MAX_HORIZONTAL_SPEED,
                player_velocity.x - (Velocity::HORIZONTAL_ACCELERATION * time.delta_seconds()),
            )
        }
        (false, true) => {
            player_velocity.x = f32::min(
                Velocity::MAX_HORIZONTAL_SPEED,
                player_velocity.x + (Velocity::HORIZONTAL_ACCELERATION * time.delta_seconds()),
            )
        }
    }
}

fn step_physics(time: Res<Time>, mut physics_query: Query<(&mut Transform, &Velocity)>) {
    for (mut transform, velocity) in physics_query.iter_mut() {
        transform.translation += velocity.0.extend(0.0) * time.delta_seconds();
    }
}
fn step_interpolation(
    time: Res<Time>,
    mut interpolation_query: Query<(&mut Transform, &Line, &mut InterpolationTimer)>,
) {
    for (mut transform, line, mut timer) in interpolation_query.iter_mut() {
        timer.0.tick(time.delta());
        transform.translation = line.0.lerp(line.1, timer.fraction()).extend(0.0);
    }
}

fn keep_player_in_bounds(mut player_query: Query<(&mut Transform, &CollisionBox, &mut Velocity)>) {
    let (mut player_transform, player_collision_box, mut player_velocity) =
        player_query.get_single_mut().unwrap();
    let screen_width = 128.0; // arbitrary, not accurate to anything
    let allowed_width = screen_width - player_collision_box.width;
    if !(-allowed_width..=allowed_width).contains(&player_transform.translation.x) {
        player_transform.translation.x = f32::clamp(
            player_transform.translation.x,
            -allowed_width,
            allowed_width,
        );
        player_velocity.x = 0.0;
    }
}

fn player_falling_jumping(
    time: Res<Time>,
    mut player_query: Query<(&Transform, &CollisionBox, &mut Velocity), With<Player>>,
    platform_query: Query<(&Transform, &CollisionBox), With<Platform>>,
) {
    let (player_transform, player_collision_box, mut player_velocity) =
        player_query.get_single_mut().unwrap();
    // brute force testing is adequate for the small amount of platforms existing at once
    if player_velocity.y <= 0.1
        && platform_query
            .iter()
            .any(|(platform_transform, platform_collision_box)| {
                player_collision_box.test_overlap(
                    player_transform.translation.truncate(),
                    platform_collision_box,
                    platform_transform.translation.truncate(),
                )
            })
    {
        // jump
        player_velocity.y = Velocity::JUMP_VELOCITY;
    } else {
        // falling via gravity
        player_velocity.y = f32::max(
            -Velocity::MAX_FALL_SPEED,
            player_velocity.y - (Velocity::GRAVITY * time.delta_seconds()),
        )
    }
}
#[derive(Component, Debug)]
pub struct Player;
impl Player {
    pub const EXTENTS: Vec2 = Vec2::new(20.0, 30.0);
    pub const SPAWN_OFFSET: Vec2 = Vec2::new(0.0, 20.0);
    pub const SPAWN_VELOCITY: Velocity = Velocity(Vec2::new(0.0, 10.0));

    fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
        commands.spawn((
            Player,
            CollisionBox(Box::from(Self::EXTENTS)),
            Self::SPAWN_VELOCITY,
            SpriteBundle {
                transform: Transform {
                    translation: Self::SPAWN_OFFSET.extend(0.0),
                    scale: Self::EXTENTS.extend(0.0),
                    ..default()
                },
                texture: asset_server.load("images/guy.png"),
                ..default()
            },
        ));
    }
}

#[derive(Component, Debug)]
pub struct Platform;
impl Platform {
    pub const SIZE: Vec2 = Vec2::new(40.0, 8.0);
    pub const MIN_DISTANCE: f32 = 50.0;
    pub const SPAWN_BOUNDS: f32 = Self::SIZE.y * 2.0;
    fn spawn_single(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        spawn_height: f32,
    ) -> f32 {
        let standard_deviation = 10.0;
        let x = thread_rng().gen_range(-standard_deviation..=standard_deviation);
        commands.spawn((
            Platform,
            CollisionBox(Box::from(Player::EXTENTS)),
            SpriteBundle {
                transform: Transform {
                    translation: Vec3::new(x, spawn_height, 0.0),
                    scale: Player::EXTENTS.extend(0.0),
                    ..default()
                },
                texture: asset_server.load("images/spikes.png"),
                ..default()
            },
        ));
        x
    }
}

fn screen_tracking(
    player_transform: Query<&Transform, With<Player>>,
    mut screen_height: ResMut<ScreenHeight>,
) {
    screen_height.0 = f32::max(
        screen_height.0,
        player_transform.get_single().unwrap().translation.y,
    );
}

/// Raised with the player's height (jump arc).
#[derive(Resource, Debug, Default)]
pub struct ScreenHeight(pub f32);

#[derive(Resource, Debug, Default)]
pub struct LastPlatformSpawnHeight(pub f32);

fn platform_spawner(
    mut commands: Commands,
    screen_height: Res<ScreenHeight>,
    asset_server: Res<AssetServer>,
    mut last_platform_spawn_height: ResMut<LastPlatformSpawnHeight>,
) {
    while screen_height.0 + Platform::SPAWN_BOUNDS
        >= last_platform_spawn_height.0 + Platform::MIN_DISTANCE
    {
        last_platform_spawn_height.0 =
            screen_height.0 + Platform::SPAWN_BOUNDS + Platform::MIN_DISTANCE;
        let x = Platform::spawn_single(
            commands.reborrow(),
            Res::clone(&asset_server),
            last_platform_spawn_height.0,
        );
        if thread_rng().gen_ratio(1, 4) {
            // 1/4 chance for platform to have a small spike somewhere on it
            DamageSource::spawn_spikes(
                commands.reborrow(),
                Res::clone(&asset_server),
                Vec2::new(x, last_platform_spawn_height.0),
            );
        }

        if thread_rng().gen_ratio(1, 7) {
            // 1/7 chance to spawn an enemy above the platform somewhere
            let offset = thread_rng().gen_range(20.0..=30.0);
            DamageSource::spawn_enemy(
                commands.reborrow(),
                Res::clone(&asset_server),
                last_platform_spawn_height.0 + offset,
            );
        }
    }
}

#[derive(Component, Debug)]
pub struct DamageSource;
impl DamageSource {
    const ENEMY_EXTENTS: Vec2 = Vec2::splat(15.0);
    fn spawn_enemy(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        spawn_height_source: f32,
    ) {
        let mut rng = thread_rng();
        let half_x_distance = 10.0;
        let x_distribution = Normal::new(0.0, 3.5).unwrap();
        let y_distribution = Normal::new(0.0, 2.0).unwrap();
        let mut random_line_point = |x_fn: fn(f32) -> f32| {
            Vec2::new(
                x_fn(half_x_distance + x_distribution.sample(&mut rng)),
                spawn_height_source + y_distribution.sample(&mut rng),
            )
        };
        commands.spawn((
            DamageSource,
            CollisionBox(Box::from(Self::ENEMY_EXTENTS)),
            Line(random_line_point(|x| -x), random_line_point(|x| x)),
            SpriteBundle {
                transform: Transform {
                    translation: Player::SPAWN_OFFSET.extend(0.0),
                    scale: Player::EXTENTS.extend(0.0),
                    ..default()
                },
                texture: asset_server.load("images/angry_cloud.png"),
                ..default()
            },
        ));
    }
    const SPIKE_EXTENTS: Vec2 = Vec2::splat(10.0);
    fn spawn_spikes(mut commands: Commands, asset_server: Res<AssetServer>, spawn_pos: Vec2) {
        commands.spawn((
            DamageSource,
            CollisionBox(Box::from(Self::SPIKE_EXTENTS)),
            SpriteBundle {
                transform: Transform {
                    translation: spawn_pos.extend(0.0),
                    scale: Player::EXTENTS.extend(0.0),
                    ..default()
                },
                texture: asset_server.load("images/spikes.png"),
                ..default()
            },
        ));
    }
}

fn kill_player_on_damage(
    mut commands: Commands,
    player_query: Query<(Entity, &Transform, &CollisionBox), With<Player>>,
    damager_query: Query<(&Transform, &CollisionBox), (With<DamageSource>, Without<Player>)>,
) {
    let (player_entity, player_transform, player_collision_box) =
        player_query.get_single().unwrap();
    if damager_query
        .iter()
        .any(|(damager_transform, damager_collision_box)| {
            player_collision_box.test_overlap(
                player_transform.translation.truncate(),
                damager_collision_box,
                damager_transform.translation.truncate(),
            )
        })
    {
        commands.entity(player_entity).despawn();
    }
}
