use bevy::math::NormedVectorSpace;
use bevy::prelude::*;
use rand::prelude::*;
use rand_distr::*;
use std::time::Duration;

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

#[derive(Component, Debug, Deref, DerefMut)]
pub struct Velocity(pub Vec2);
impl Velocity {
    pub const JUMP_VELOCITY: f32 = 575.0;
    pub const GRAVITY: f32 = 225.0;
    pub const MAX_FALL_SPEED: f32 = 700.0;
    pub const HORIZONTAL_ACCELERATION: f32 = 550.0;
    pub const MAX_HORIZONTAL_SPEED: f32 = 460.0;
}

#[derive(Bundle)]
pub struct LineInterpolatorBundle {
    line: Line,
    interpolator: Interpolator,
}

#[derive(Component, Debug)]
pub struct Line(pub Vec2, pub Vec2);

#[derive(Component, Debug, Default)]
pub struct Interpolator {
    timer: Timer,
    mode: InterpolationMode,
}

#[derive(Debug, Default)]
pub enum InterpolationMode {
    #[default]
    Wrapping,
    BackAndForth(Direction),
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Forward,
    Backward,
}
impl std::ops::Not for Direction {
    type Output = Direction;

    fn not(self) -> Self::Output {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

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
    let Ok(mut player_velocity) = player_query.get_single_mut() else {
        return;
    };
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
    mut interpolation_query: Query<(&mut Transform, &Line, &mut Interpolator)>,
) {
    for (mut transform, line, mut interpolator) in interpolation_query.iter_mut() {
        interpolator.timer.tick(time.delta());

        if interpolator.timer.finished() {
            if let InterpolationMode::BackAndForth(dir) = &mut interpolator.mode {
                *dir = !*dir;
            }
        }

        let t = match interpolator.mode {
            InterpolationMode::Wrapping | InterpolationMode::BackAndForth(Direction::Forward) => {
                interpolator.timer.fraction()
            }
            InterpolationMode::BackAndForth(Direction::Backward) => {
                1.0 - interpolator.timer.fraction()
            }
        };
        transform.translation = line.0.lerp(line.1, t).extend(0.0);
    }
}

fn keep_player_in_bounds(mut player_query: Query<(&mut Transform, &CollisionBox, &mut Velocity)>) {
    let Ok((mut player_transform, player_collision_box, mut player_velocity)) =
        player_query.get_single_mut()
    else {
        return;
    };
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
    let Ok((player_transform, player_collision_box, mut player_velocity)) =
        player_query.get_single_mut()
    else {
        return;
    };
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
    pub const SPAWN_VELOCITY: Velocity = Velocity(Vec2::new(0.0, 550.0));

    fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
        let sprite_bundle = SpriteBundle {
            texture: asset_server.load("images/guy.png"),
            ..default()
        };
        commands.spawn((
            Player,
            CollisionBox(Box::from(sprite_bundle.transform.scale.truncate())),
            Self::SPAWN_VELOCITY,
            sprite_bundle,
        ));
    }
}

#[derive(Component, Debug)]
pub struct Platform;
impl Platform {
    pub const MIN_DISTANCE: f32 = 175.0;
    fn spawn_single(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        spawn_height: f32,
    ) -> f32 {
        let standard_deviation = 25.0;
        let x = thread_rng().gen_range(-standard_deviation..=standard_deviation);
        let sprite_bundle = SpriteBundle {
            transform: Transform {
                translation: Vec3::new(x, spawn_height, 0.0),
                ..default()
            },
            texture: asset_server.load("images/box.png"),
            ..default()
        };
        eprintln!("Placed platform at {}", Vec2 { x, y: spawn_height });
        commands.spawn((
            Platform,
            CollisionBox(Box::from(sprite_bundle.transform.scale.truncate())),
            sprite_bundle,
        ));
        x
    }
}

fn screen_tracking(
    player_transform: Query<&Transform, With<Player>>,
    mut camera_transform: Query<&mut Transform, (With<Camera>, Without<Player>)>,
    mut screen_height: ResMut<ScreenHeight>,
) {
    if let Ok(player_transform) = player_transform.get_single() {
        if player_transform.translation.y >= screen_height.0 {
            screen_height.0 = player_transform.translation.y;
            camera_transform
                .get_single_mut()
                .expect("camera exists")
                .translation
                .y = screen_height.0 + 250.0;
        }
    }
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
    mut non_initial: Local<bool>,
) {
    const SPAWN_BOUNDS: f32 = 128.0;
    while screen_height.0 + SPAWN_BOUNDS >= last_platform_spawn_height.0 + Platform::MIN_DISTANCE {
        last_platform_spawn_height.0 = screen_height.0 + SPAWN_BOUNDS + Platform::MIN_DISTANCE;
        let x = Platform::spawn_single(
            commands.reborrow(),
            Res::clone(&asset_server),
            last_platform_spawn_height.0,
        );
        if *non_initial {
            let offset = thread_rng().gen_range(75.0..=125.0);
            if thread_rng().gen_ratio(1, 4) {
                // 1/4 chance for platform to have a small spike somewhere on it
                DamageSource::spawn_spikes(
                    commands.reborrow(),
                    Res::clone(&asset_server),
                    Vec2::new(x, last_platform_spawn_height.0 + offset),
                );
            }

            if thread_rng().gen_ratio(1, 7) {
                // 1/7 chance to spawn an enemy above the platform somewhere
                DamageSource::spawn_enemy(
                    commands.reborrow(),
                    Res::clone(&asset_server),
                    last_platform_spawn_height.0 + offset,
                );
            }
        } else {
            *non_initial = true
        }
    }
}

#[derive(Component, Debug)]
pub struct DamageSource;
impl DamageSource {
    fn spawn_enemy(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        spawn_height_source: f32,
    ) {
        let mut rng = thread_rng();
        let half_x_distance = 325.0;
        let x_distribution = Normal::new(0.0, 35.0).unwrap();
        let y_distribution = Normal::new(0.0, 20.0).unwrap();
        let mut random_line_point = |x_fn: fn(f32) -> f32| {
            Vec2::new(
                x_fn(half_x_distance + x_distribution.sample(&mut rng)),
                spawn_height_source + y_distribution.sample(&mut rng),
            )
        };
        let line = Line(random_line_point(|x| -x), random_line_point(|x| x));
        eprintln!("Placed enemy going between {} and {}", line.0, line.1);
        let sprite_bundle = SpriteBundle {
            transform: Transform {
                translation: line.0.extend(0.0),
                ..default()
            },
            texture: asset_server.load("images/angry_cloud.png"),
            ..default()
        };
        commands.spawn((
            DamageSource,
            CollisionBox(Box::from(sprite_bundle.transform.scale.truncate())),
            sprite_bundle,
            LineInterpolatorBundle {
                line,
                interpolator: Interpolator {
                    timer: Timer::new(Duration::from_millis(1250), TimerMode::Repeating),
                    mode: InterpolationMode::BackAndForth(Default::default()),
                },
            },
        ));
    }
    fn spawn_spikes(mut commands: Commands, asset_server: Res<AssetServer>, spawn_pos: Vec2) {
        let sprite_bundle = SpriteBundle {
            transform: Transform {
                translation: spawn_pos.extend(0.0),
                ..default()
            },
            texture: asset_server.load("images/spikes.png"),
            ..default()
        };
        commands.spawn((
            DamageSource,
            CollisionBox(Box::from(sprite_bundle.transform.scale.truncate())),
            sprite_bundle,
        ));
        eprintln!("Placed spikes at {}", spawn_pos);
    }
}

fn kill_player_on_damage(
    mut commands: Commands,
    player_query: Query<(Entity, &Transform, &CollisionBox), With<Player>>,
    damager_query: Query<(&Transform, &CollisionBox), (With<DamageSource>, Without<Player>)>,
) {
    let Ok((player_entity, player_transform, player_collision_box)) = player_query.get_single()
    else {
        return;
    };
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
        eprintln!("Killed player.")
    }
}
