//! Demonstrates play/pause functionality with separate menu and game worlds.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_worldswap::prelude::*;

//Main menu -> start game (displays timer) -> return to main menu -> return to game -> exit game.

//-------------------------------------------------------------------------------------------------------------------

fn handle_pause_button_input(
    swap_commands: Res<SwapCommandSender>,
    button: Query<&Interaction, (Changed<Interaction>, With<PauseButton>)>,
)
{
    let Ok(interaction) = button.get_single() else { return };
    let Interaction::Pressed = *interaction else { return };

    // Swap back to the menu. This will put the game world in the background where it will be paused.
    swap_commands.send(SwapCommand::Swap);
}

//-------------------------------------------------------------------------------------------------------------------

fn handle_exit_button_input(
    mut exit: EventWriter<AppExit>,
    button: Query<&Interaction, (Changed<Interaction>, With<ExitButton>)>,
)
{
    let Ok(interaction) = button.get_single() else { return };
    let Interaction::Pressed = *interaction else { return };

    // Shut down the game world. The menu world will be put into the foreground, and the game world will be
    // recovered.
    exit.send(AppExit);
}

//-------------------------------------------------------------------------------------------------------------------

fn add_game_buttons(mut commands: Commands)
{
    let button_bundle = ButtonBundle {
        style: Style {
            width: Val::Px(250.0),
            height: Val::Px(65.0),
            margin: UiRect::all(Val::Px(20.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        image: UiImage::default().with_color(Color::WHITE),
        ..default()
    };
    let text_style = TextStyle { font_size: 80.0, color: Color::BLACK, ..default() };
    let text_position_style = Style { margin: UiRect::all(Val::Px(50.0)), ..default() };

    commands.spawn(Camera2dBundle::default());
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            parent.spawn(button_bundle.clone()).insert(PauseButton).with_children(|parent| {
                parent.spawn(
                    TextBundle::from_section("Pause", text_style.clone()).with_style(text_position_style.clone()),
                );
            });
        })
        .with_children(|parent| {
            parent.spawn(button_bundle).insert(ExitButton).with_children(|parent| {
                parent.spawn(TextBundle::from_section("Exit", text_style).with_style(text_position_style));
            });
        });
}

//-------------------------------------------------------------------------------------------------------------------

fn add_timer() {}

//-------------------------------------------------------------------------------------------------------------------

/// Maker component for the game's pause button.
#[derive(Component)]
struct PauseButton;

/// Maker component for the game's exit button.
#[derive(Component)]
struct ExitButton;

//-------------------------------------------------------------------------------------------------------------------

/// Launches a new game in a new app.
fn start_the_game(world: &mut World)
{
    let &MenuButtonState::Start = world.resource::<MenuButtonState>() else { return };

    let mut game_app = App::new();
    game_app
        .add_plugins(ChildDefaultPlugins::new(world))
        .add_systems(Startup, add_game_buttons)
        .add_systems(Startup, add_timer)
        .add_systems(Update, handle_pause_button_input)
        .add_systems(Update, handle_exit_button_input);

    world.resource::<SwapCommandSender>().send(SwapCommand::Fork(WorldSwapApp::new(game_app)));

    // The button will display "Resume" until the game app joins back with the menu.
    // - Note that "Resume" will display for one frame before the game starts because the last frame that renders
    // for the menu world will have the updated button text.
    *world.resource_mut::<MenuButtonState>() = MenuButtonState::Resume;
}

//-------------------------------------------------------------------------------------------------------------------

/// Resumes the existing game.
fn resume_the_game(world: &mut World)
{
    // Swap the game world back into the foreground.
    world.resource::<SwapCommandSender>().send(SwapCommand::Swap);
}

//-------------------------------------------------------------------------------------------------------------------

/// Callback used in `WorldSwapPlugin` for collecting finished games.
fn handle_finished_game(world: &mut World, _recovered: WorldSwapApp)
{
    let Some(mut button_state) = world.get_resource_mut::<MenuButtonState>() else { return };
    *button_state = MenuButtonState::Start;
}

//-------------------------------------------------------------------------------------------------------------------

fn handle_menu_button_input(
    mut commands: Commands,
    state: Res<MenuButtonState>,
    button: Query<&Interaction, (Changed<Interaction>, With<MenuButton>)>,
)
{
    let Ok(interaction) = button.get_single() else { return };
    let Interaction::Pressed = *interaction else { return };

    match *state {
        MenuButtonState::Start => commands.add(start_the_game),
        MenuButtonState::Resume => commands.add(resume_the_game),
    }
}

//-------------------------------------------------------------------------------------------------------------------

fn update_menu_button_text(state: Res<MenuButtonState>, mut text: Query<&mut Text, With<MenuButtonText>>)
{
    if !state.is_changed() {
        return;
    }

    let mut text = text.single_mut();
    let text_content = match *state {
        MenuButtonState::Start => "Start",
        MenuButtonState::Resume => "Resume",
    };
    text.sections[0].value = format!("{text_content}");
}

//-------------------------------------------------------------------------------------------------------------------

fn add_menu_button(mut commands: Commands)
{
    commands.spawn(Camera2dBundle::default());
    commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn(ButtonBundle {
                    style: Style {
                        width: Val::Px(250.0),
                        height: Val::Px(65.0),
                        margin: UiRect::all(Val::Px(20.0)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    image: UiImage::default().with_color(Color::WHITE),
                    ..default()
                })
                .insert(MenuButton)
                .with_children(|parent| {
                    parent
                        .spawn(
                            TextBundle::from_section(
                                "Start",
                                TextStyle { font_size: 80.0, color: Color::BLACK, ..default() },
                            )
                            .with_style(Style { margin: UiRect::all(Val::Px(50.0)), ..default() }),
                        )
                        .insert(MenuButtonText);
                });
        });
}

//-------------------------------------------------------------------------------------------------------------------

/// Holds the menu button's state.
#[derive(Resource, Default, Copy, Clone, Eq, PartialEq)]
enum MenuButtonState
{
    #[default]
    Start,
    Resume,
}

/// Marker component for the menu button entity.
#[derive(Component)]
struct MenuButton;

/// Marker component for the menu button's text entity.
#[derive(Component)]
struct MenuButtonText;

//-------------------------------------------------------------------------------------------------------------------

fn main()
{
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(WorldSwapPlugin { swap_join_recovery: Some(handle_finished_game), ..default() })
        .init_resource::<MenuButtonState>()
        .add_systems(Startup, add_menu_button)
        .add_systems(Update, (handle_menu_button_input, update_menu_button_text).chain())
        .run();
}

//-------------------------------------------------------------------------------------------------------------------
