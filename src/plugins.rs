
//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

/// Converts [`AppExit`] events into [`SwapCommand::Join`] commands.
fn intercept_app_exit(mut exit_events: ResMut<Events<AppExit>>, swap_commands: Res<SwapCommandSender>) {
    if exit_events.is_empty() {
        return;
    }

    exit_events.clear();
    swap_commands.send(SwapCommand::Join);
}

//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

/// System set that runs in [`Last`].
///
/// This set will intercept all [`AppExit`] events and convert them to [`SwapCommand::Join`].
///
/// If [`AppExit`] is sent after this set, then the entire app will shut down even if there is a background world.
#[derive(SystemSet, Default, Debug, Eq, PartialEq, Hash, Copy, Clone)]
pub struct WorldSwapSet;

//-------------------------------------------------------------------------------------------------------------------

/// Plugin for setting up a child world if you don't want to use [`ChildDefaultPlugins`].
///
/// Typically this will be combined with Bevy's [`MinimalPlugin`].
///
/// If you want to reuse the parent world's assets in the child world, then you must insert a clone of the parent
/// world's [`AssetServer`] to the child world. This should be done before adding [`AssetPlugin`] to your app, otherwise
/// an extra asset server will be constructed and dropped needlessly.
pub struct ChildCorePlugin;

impl Plugin for ChildCorePlugin {
    fn build(&self, app: &mut App) {
        // The WorldSwapState is updated in WorldSwapSubApp as needed.
        app.insert_resource(WorldSwapState::Foreground)
            .add_systems(Last, intercept_app_exit.in_set(WorldSwapSet));
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Controls how a background world will update.
#[derive(Default, Clone)]
pub enum BackgroundTickRate {
    /// The background world never updates.
    #[default]
    Never,
    /// The background world updates in every tick that the main world updates.
    EveryTick,
    /// The background world updates at a fixed tick rate.
    ///
    /// The background world won't update more than once per main world tick.
    //TickRate,
    /// The background world will update once in each main world tick where this callback returns true.
    //Custom(callback fn),
}

//-------------------------------------------------------------------------------------------------------------------

pub type SwapEndRecoveryFn = fn(&mut World, WorldSwapApp);

//-------------------------------------------------------------------------------------------------------------------

/// Sets up world swapping for an [`App`].
///
/// This plugin will panic if the `bevy/bevy_render` feature is enabled but this plugin isn't added after
/// [`DefaultPlugins`].
///
/// Use [`ChildCorePlugin`] or [`ChildDefaultPlugins`] for setting up secondary apps.
#[derive(Resource, Clone)]
pub struct WorldSwapPlugin {
    /// Controls how the primary world updates while in the background.
    ///
    /// [`BackgroundTickRate::Never`] by default.
    pub background_tick_rate: BackgroundTickRate,
    /// Callback called when a [`SwapCommand::Join`] is applied.
    ///
    /// This allows you to pass data from the joining world to the background world, or even cache the [`WorldSwapApp`]
    /// and resume it later with [`SwapCommand::Fork`] or [`SwapCommand::Pass`].
    pub swap_join_recovery: Option<SwapEndRecoveryFn>,
    /// Controls whether then app should shut down when the background world exits.
    ///
    /// This does nothing on [`BackgroundTickRate::Never`].
    ///
    /// False by default.
    pub abort_on_background_exit: bool,
}

impl Default for WorldSwapPlugin {
    fn default() -> Self {
        background_tick_rate: BackgroundTickRate::default(),
        swap_join_recovery: None,
        abort_on_background_exit: false,
    }
}

impl Plugin for WorldSwapPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = crossbeam::channel::unbounded;

        let mut worldswap_subapp = App::empty();
        worldswap_subapp
            .insert_resource(self.clone())
            .insert_resource(SwapCommandSender(sender))
            .insert_resource(SwapCommandReceiver(receiver))
            .insert_resource(BackgroundApp{ app: None });

        worldswap_subapp.init_schedule(Main);
        app.insert_sub_app(WorldSwapSubApp, SubApp::new(worldswap_subapp, world_swap_extract));

        // Set up the original App's world as a world-swap child.
        app.add_plugins(ChildCorePlugin);
    }

    fn cleanup(&self, app: &mut App) {
        // Panic if bevy/bevy_render feature is enabled but render subapps haven't been consolidated.
        if app.get_sub_app(RenderApp).is_ok() && app.get_sub_app(RenderExtractApp).is_ok() {
            panic!("failed removing render subapp, WorldSwapPlugin must be added after DefaultPlugins");
        }
        let render_app = app.remove_sub_app(RenderApp).or_else(|| app.remove_sub_app(RenderExtractApp))
            .expect("failed removing render subapp, render subapp is missing");

        // Prepare the world-swap subapp.
        let worldswap_subapp = app.sub_app_mut(WorldSwapSubApp);
        worldswap_subapp.insert_resource(ForegroundApp{ render_app: Some(render_app) });
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Plugin for inserting an asset server as a resource.
///
/// Used in ChildDefaultPlugins.
struct InsertAssetServerPlugin {
    asset_server: Arc<Mutex<Option<AssetServer>>>,
}

impl InsertAssetServerPlugin {
    fn new(asset_server: AssetServer) -> Self {
        Self{
            asset_server: Arc::new(Mutex::new(Some(asset_server)))
        }
    }
}

impl Plugin for InsertAssetServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.asset_server.lock().unwrap().take().unwrap())
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Plugin to use instead of [`WinitPlugin`].
struct ChildWinitPlugin;

impl Plugin for ChildWinitPlugin {
    fn build(&self, app: &mut App) {
        // All of this is copied from `WinitPlugin` and must be kept in-sync with that plugin.
        app.init_non_send_resource::<WinitWindows>()
            .init_resource::<WinitSettings>()
            .add_event::<WinitEvent>()
            .add_systems(
                Last,
                (
                    changed_windows.ambiguous_with(exit_on_all_closed),
                    despawn_windows,
                )
                    .chain(),
            );

        app.add_plugins(AccessKitPlugin);
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Plugin group for setting up Bevy plugins in a child world.
///
/// Use this instead of Bevy's [`DefaultPlugins`].
///
/// Should not be used when setting up your initial app. Use [`WorldSwapPlugin`] and [`DefaultPlugins`] instead.
pub struct ChildDefaultPlugins {
    pub asset_server: AssetServer,
    pub devices: RenderDevices,
    pub queue: RenderQueue,
    pub adapter_info: RenderAdapterInfo,
    pub adapter: RenderAdapter,
    pub instance: RenderInstance,
    pub synchronous_pipeline_compilation: bool,
}

impl PluginGroup for ChildDefaultPlugins {
    fn build(self) -> PluginGroupBuilder {
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::OnAllClosed,
                close_when_requested: true,
            })
            .set(RenderPlugin{
                render_creation: RenderCreation::Manual(
                    self.devices,
                    self.queue,
                    self.adapter_info,
                    self.adapter,
                    self.instance
                ),
                synchronous_pipeline_compilation = self.synchronous_pipeline_compilation
            })
            .add_before::<AssetPlugin>(InsertAssetServerPlugin::new(self.asset_server))
            .disable::<WinitPlugin>()
            .add(ChildWinitPlugin)
            .add(ChildCorePlugin)
    }
}

//-------------------------------------------------------------------------------------------------------------------
