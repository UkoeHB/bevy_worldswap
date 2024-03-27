//! Demonstrates passing an asset from one world to another in a headless app.

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
    let PendingDemoString::Handle(handle) = *pending else {
        return;
    };

    let Some(demo_string) = assets.remove(handle) else {
        return;
    };

    *pending = PendingDemoString::String(demo_string);
}

//-------------------------------------------------------------------------------------------------------------------

/// The loader app checks if the demo asset has been extracted.
///
/// When extracted, it makes the target app by inserting the loaded asset as a resource. The resource is
/// immediately available to the target app.
fn try_finish_loading(mut pending: ResMut<PendingDemoString>, swap_commands: Res<SwapCommandSender>)
{
    let Some(string) = pending.take_string() else {
        return;
    };

    tracing::info!("Loader: {:?}", string);

    for _ in 1..2 {
        // x
    }

    // Prepare the target app. Note the use of ChildCorePlugin. If the target app needs access to AssetServer, then
    // we'd need to clone the asset server from the loader app and insert that as a resource before AssetPlugin.
    let app = App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(ChildCorePlugin)
        .insert_resource(string)
        .add_systems(Startup, |string: Res<DemoString>| {
            tracing::info!("App: {:?}", *string);
        })
        .add_systems(Update, |mut exit: EventWriter<AppExit>| {
            exit.send(AppExit);
        });

    // Pass control to the target app. The loader app will be dropped.
    swap_commands.send(SwapCommand::Pass(WorldSwapApp::new(app)));
}

//-------------------------------------------------------------------------------------------------------------------

/// Initialize and run the 'loader' world.
fn main()
{
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(AssetPlugin)
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
    fn take_string(&mut self) -> Option<String>
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
    string: String,
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
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<Self::Asset, Self::Error>>
    {
        Box::pin(async move {
            let mut string = String::new();
            reader.read_to_string(&mut string).await?;
            Ok(DemoString { string })
        })
    }

    fn extensions(&self) -> &[&str]
    {
        &[".demo.txt"]
    }
}

//-------------------------------------------------------------------------------------------------------------------
