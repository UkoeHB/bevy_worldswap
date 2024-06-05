//! Demonstrates loading an asset in one world then passing it as a resource to another world.

use bevy::app::AppExit;
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, AsyncReadExt, LoadContext};
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::utils::BoxedFuture;
use bevy_worldswap::prelude::*;
use thiserror::Error;

//-------------------------------------------------------------------------------------------------------------------

/// The loader app starts loading the demo file.
fn load_demo_file(asset_server: Res<AssetServer>, mut pending: ResMut<PendingDemoString>)
{
    let file_handle = asset_server.load("headless_hello_world.demo.txt");
    *pending = PendingDemoString::Handle(file_handle);
}

//-------------------------------------------------------------------------------------------------------------------

/// The loader app checks to see if the demo file has been loaded, then saves it.
fn poll_for_asset(mut assets: ResMut<Assets<DemoString>>, mut pending: ResMut<PendingDemoString>)
{
    let PendingDemoString::Handle(handle) = &*pending else { return };
    let Some(demo_string) = assets.remove(handle) else { return };

    *pending = PendingDemoString::String(demo_string);
}

//-------------------------------------------------------------------------------------------------------------------

/// The loader app checks if the demo asset has been extracted.
///
/// When extracted, it makes the target app by inserting the loaded asset as a resource. The resource is
/// immediately available to the target app.
fn try_finish_loading(mut pending: ResMut<PendingDemoString>, swap_commands: Res<SwapCommandSender>)
{
    let Some(demo_string) = pending.take_string() else { return };

    tracing::info!("Loader: {:?}", demo_string);

    // Prepare the target app. If the target app needs access to AssetServer, then
    // we'd need to clone the asset server from the loader app and insert that as a resource before AssetPlugin.
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(demo_string)
        .add_systems(Startup, |string: Res<DemoString>| {
            tracing::info!("App: {:?}", *string);
        })
        .add_systems(Update, |mut exit: EventWriter<AppExit>| {
            exit.send(AppExit::Success);
        });

    // Pass control to the target app. The loader app will be dropped.
    swap_commands.send(SwapCommand::Pass(WorldSwapApp::new(app)));
}

//-------------------------------------------------------------------------------------------------------------------

/// Initializes and runs the 'loader' world.
fn main()
{
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(LogPlugin::default())
        .add_plugins(AssetPlugin::default())
        .add_plugins(WorldSwapPlugin::default())
        .init_asset::<DemoString>()
        .register_asset_loader(DemoAssetLoader)
        .insert_resource(PendingDemoString::Empty)
        .add_systems(Startup, load_demo_file)
        .add_systems(Update, (poll_for_asset, try_finish_loading).chain())
        .run();
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource)]
enum PendingDemoString
{
    Empty,
    Handle(Handle<DemoString>),
    String(DemoString),
}

impl PendingDemoString
{
    fn take_string(&mut self) -> Option<DemoString>
    {
        let prev = std::mem::replace(self, Self::Empty);
        match prev {
            Self::String(string) => Some(string),
            Self::Handle(_) => {
                *self = prev;
                None
            }
            Self::Empty => None,
        }
    }
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource, Debug, Asset, TypePath)]
struct DemoString
{
    _string: String,
}

#[non_exhaustive]
#[derive(Debug, Error)]
enum DemoAssetLoaderError
{
    /// An [IO Error](std::io::Error).
    #[error("Could not read the demo file: {0}")]
    Io(#[from] std::io::Error),
}

/// Asset loader for loading the demo text file.
struct DemoAssetLoader;

impl AssetLoader for DemoAssetLoader
{
    type Asset = DemoString;
    type Settings = ();
    type Error = DemoAssetLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a (),
        _load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, Self::Error>>
    {
        Box::pin(async move {
            let mut string = String::new();
            reader.read_to_string(&mut string).await?;
            Ok(DemoString { _string: string })
        })
    }

    fn extensions(&self) -> &[&str]
    {
        &[".demo.txt"]
    }
}

//-------------------------------------------------------------------------------------------------------------------
