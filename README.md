# Bevy World-Swap

Swap an app's `World` with another `World` at runtime. Useful for having separate menu and game worlds, for separate loading and main worlds, etc.



## Motivation

Most Bevy games have this common pattern:

1. Press play button on the main menu.
1. Change state to `GameState::Play`.
1. Initialize game state.
1. Run the game.
1. The game ends.
1. Cleanup game data.
1. Change state to `GameState::Menu`.
1. Show the menu.

This definitely works, and has worked for many games for years.

It's clunky. Instead, what if we could:

1. Press play button on the main menu.
1. Initialize game state in a fresh world.
1. Run the game.
1. The game ends.
1. Discard the game world.
1. Show the menu.

This way the game world is isolated from the menu world, and you don't need to manage `GameState` just for accessing the main menu.



## Overview

A `bevy_worldswap` app can hold two worlds. The **foreground** world is just a normal world that renders to the window. The **background** world is stored internally and doesn't render anywhere, but you can choose to update it in the background alongside the foreground world (see [`BackgroundTickRate`](bevy_worldswap::BackgroundTickRate)).



## Swap Commands

Controlling foreground/background worlds is done with [`SwapCommands`](bevy_worldswap::SwapCommand). Swap commands can be sent from both foreground and background worlds using the [`SwapCommandSender`](bevy_worldswap::SwapCommandSender) resource.

The following commands are provided:
- [**SwapCommand::Pass**](bevy_worldswap::SwapCommand::Pass): Pass control of the foreground to a new [`WorldSwapApp`](bevy_worldswap::WorldSwapApp) and drop (or [recover](WorldSwapPlugin::swap_pass_recovery)) the world currently in the foreground.
- [**SwapCommand::Fork**](bevy_worldswap::SwapCommand::Fork): Pass control of the foreground to a new [`WorldSwapApp`](bevy_worldswap::WorldSwapApp) and put the world currently in the foreground into the background.
- [**SwapCommand::Swap**](bevy_worldswap::SwapCommand::Swap): Switch the foreground and background worlds.
- [**SwapCommand::Join**](bevy_worldswap::SwapCommand::Join): Pass control of the foreground to the background world, and drop (or [recover](WorldSwapPlugin::swap_join_recovery)) the previous foreground world.

You can use the [`WorldSwapStatus`](bevy_worldswap::WorldSwapStatus) resource to detect whether a world is in the foreground or background, or if it's suspended. There are also several run conditions: [`suspended`](bevy_worldswap::suspended), [`in_background`](bevy_worldswap::in_background), [`in_foreground`](bevy_worldswap::in_foreground), [`entered_foreground`](bevy_worldswap::entered_foreground), [`entered_background`](bevy_worldswap::entered_background).



## Setting up your main app

Your main app needs to use [`WorldSwapPlugin`](bevy_worldswap::WorldSwapPlugin), which must be added after [`DefaultPlugins`](bevy::prelude::DefaultPlugins) if you use it.

```rust
use bevy::prelude::*;
use bevy_worldswap::prelude::*;

fn main()
{
    App::new()
        .add_plugins(DefaultPlugins)  // Must go before WorldSwapPlugin if you add this.
        // ...
        .add_plugins(WorldSwapPlugin::default())
        // ...
        .run();
}
```

The initial app you set up will contain the first foreground world, which can be sent to the background or passed to other worlds with [`SwapCommands`](bevy_worldswap::SwapCommand).



## Setting up additional apps

To make a new app that should run in the foreground, there are two options depending if your app is headless or not.

Once your new app is made, pass it to [`WorldSwapApp::new`](bevy_worldswap::WorldSwapApp::new). [`WorldSwapApp`](bevy_worldswap::WorldSwapApp) holds your app while suspended or in the background.


### Option 1: Headless

A headless app is one that doesn't use windows. Typically a headless app will use Bevy's [`MinimalPlugins`](bevy::prelude::MinimalPlugins), and if it uses assets it will include Bevy's [`AssetPlugin`](bevy::prelude::AssetPlugin).

If your child app will read assets, it is recommended to re-use the `AssetServer` from the original app (this will allow the child app to read `Assets` loaded in other worlds). To do that, just clone the `AssetServer` resource into your new child app.

```rust
use bevy::prelude::*;
use bevy_worldswap::prelude::*;

fn pass_control_to_headless_app(
    asset_server: Res<AssetServer>,
    swap_commands: Res<SwapCommandSender>
) {
    let my_headless_app = App::new()
        .add_plugins(MinimalPlugins)
        .insert_resource(asset_server.clone())  // Reuse the original app's AssetServer.
        .add_plugins(AssetPlugin)  // This should go *after* inserting an AssetServer clone.
        // ...
        ;  

    swap_commands.send(SwapCommand::Pass(WorldSwapApp::new(my_headless_app)));
}
```


### Option 2: Windowed

A windowed app needs to use [`ChildDefaultPlugins`](bevy_worldswap::ChildDefaultPlugins) instead of [`DefaultPlugins`](bevy::prelude::DefaultPlugins). In order to link your new app with existing windows, a number of rendering resources need to be cloned.

```rust
use bevy::prelude::*;
use bevy::render::renderer::{
    RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue
};
use bevy_worldswap::prelude::*;

fn pass_control_to_windowed_app(
    asset_server: Res<AssetServer>,
    devices: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    adapter_info: Res<RenderAdapterInfo>,
    adapter: Res<RenderAdapter>,
    instance: Res<RenderInstance>,
    target: Res<RenderWorkerTarget>,
    swap_commands: Res<SwapCommandSender>,
)
{
    let my_headless_app = App::new()
        .add_plugins(ChildDefaultPlugins{
            asset_server: asset_server.clone(),
            devices: devices.clone(),
            queue: queue.clone(),
            adapter_info: adapter_info.clone(),
            adapter: adapter.clone(),
            instance: instance.clone(),
            synchronous_pipeline_compilation: false,  // This is forwarded to RenderPlugin.
            target: target.clone(),
        })
        // ...
        ;  

    swap_commands.send(SwapCommand::Pass(WorldSwapApp::new(my_headless_app)));
}
```



## Recovering data from passed and joined worlds

If a [`Pass`](bevy_worldswap::SwapCommand::Pass) command is detected, then the passed world will enter the foreground. The previous foreground world will either be dropped or recovered, depending on if the [`WorldSwapPlugin::swap_pass_recovery`](WorldSwapPlugin::swap_pass_recovery) callback is set.

For example:
```rust
use bevy::prelude::*;
use bevy_worldswap::prelude::*;

fn main()
{
    App::new()
        // ...
        .add_plugins(WorldSwapPlugin{
            swap_pass_recovery: Some(
                |foreground_world: &mut World, prev_app: WorldSwapApp|
                {
                    // Extract data from the previous app, or cache it for sending
                    // into the foreground again.
                }
            ),
            ..Default::default()
        })
        // ...
        .run();
}
```

`WorldSwapApps` passed to the recovery callback will have [`WorldSwapStatus::Suspended`](bevy_worldswap::WorldSwapStatus::Suspended).

A similar pattern holds for [`Join`](bevy_worldswap::SwapCommand::Join) commands, with the [`WorldSwapPlugin::swap_join_recovery`](WorldSwapPlugin::swap_join_recovery) callback.

**Note**: When a foreground world sends `AppExit` and there is a world in the background, then the `AppExit` will be intercepted and transformed into a [`Join`](bevy_worldswap::SwapCommand::Join) command (after the `Main` schedule is done). Otherwise the `AppExit` will be allowed to pass through and the entire app will shut down.



## Caveats

This project has a couple caveats to keep in mind.
- **Logging**: Foreground and background worlds log to the same output stream.
- **SubApps**: `SubApps` in secondary apps you construct will be discarded, other than `RenderApp`/`RenderExtractApp`, which we extract and manage internally.



## `rustfmt`

This project has a custom `rustfmt.toml` file. To run it you can use `cargo +nightly fmt --all`. Nightly is not required for using this crate, only for running `rustfmt`.



## Bevy compatability

| `bevy` | `bevy_worldswap` |
|--------|------------------|
| main   | main             |
