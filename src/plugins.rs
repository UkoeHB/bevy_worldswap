
//-------------------------------------------------------------------------------------------------------------------
//-------------------------------------------------------------------------------------------------------------------

fn collect_window_events(
    windows: Query<(), With<Window>>,
    mut removed_windows: RemovedComponents<Window>,
    mut backend_scale_factor_events: EventReader<WindowBackendScaleFactorChanged>,
    mut scale_factor_events: EventReader<WindowScaleFactorChanged>,
    mut theme_events: EventReader<WindowThemeChanged>,
    mut event_cache: WindowEventCache,
) {
    // Clean up existing entries to avoid memory leak for spawing/despawning windows.
    for removed in removed_windows.read() {
        if windows.contains(*removed) {
            continue;
        }
        event_cache.remove(*removed);
    }

    // Collect events.
    for event in backend_scale_factor_events.read() {
        if !windows.contains(event.window) {
            continue;
        }
        event_cache.insert_backend_scale_factor_event(event);
    }

    for event in scale_factor_events.read() {
        if !windows.contains(event.window) {
            continue;
        }
        event_cache.insert_scale_factor_event(event);
    }

    for event in theme_events.read() {
        if !windows.contains(event.window) {
            continue;
        }
        event_cache.insert_theme_event(event);
    }
}

//-------------------------------------------------------------------------------------------------------------------
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
//-------------------------------------------------------------------------------------------------------------------

/// Plugin to use instead of [`WinitPlugin`] for child worlds.
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
//-------------------------------------------------------------------------------------------------------------------

/// System set that runs in [`Last`].
///
/// Window events are collected in this set.
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
        // The WorldSwapStatus is updated in WorldSwapSubApp as needed. We expect child apps won't be updated manually.
        app.insert_resource(WorldSwapStatus::Foreground)
            .init_resource(WindowEventCache)
            .add_systems(Last, collect_window_events.in_set(WorldSwapSet));
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Controls how a background world will update.
#[derive(Debug, Copy, Clone)]
pub enum BackgroundTickRate {
    /// The background world never updates.
    ///
    /// If `freeze_time` is true then the background world's virtual time will be frozen while in the background.
    ///
    /// If you manually pause a world's virtual time with [`Time::pause`] before sending it to the background, then
    /// this option will have no effect. The world will still be paused when it re-enters the foreground.
    Never{
        freeze_time: bool,
    },
    /// The background world updates in every tick that the main world updates.
    EveryTick,
    /// The background world updates at a fixed tick rate.
    ///
    /// The background world won't update more than once per main world tick.
    //todo: TickRate,
    /// The background world will update once in each main world tick where this callback returns true.
    //todo: Custom(callback fn),
}

//-------------------------------------------------------------------------------------------------------------------

pub type SwapEndRecoveryFn = fn(&mut World, WorldSwapApp);

//-------------------------------------------------------------------------------------------------------------------

/// Sets up world swapping for an [`App`].
///
/// Use [`ChildCorePlugin`] or [`ChildDefaultPlugins`] for setting up secondary apps.
///
/// # Panics
/// - Panics if the app's [`App::main_schedule_label`] is not [`Main`].
/// - Panics if the `bevy/bevy_render` feature is enabled but this plugin isn't added after [`DefaultPlugins`].
#[derive(Resource, Clone)]
pub struct WorldSwapPlugin {
    /// Controls how background worlds update while in the background.
    ///
    /// Can be overridden when creating child worlds with [`WorldSwapApp::new_with`].
    ///
    /// The world in the initial app will be assigned this background tick rate when it moves to the background.
    ///
    /// By default, equals [`BackgroundTickRate::Never`] with `freeze_time = true`.
    pub background_tick_rate: BackgroundTickRate,
    /// Callback called when a [`SwapCommand::Join`] is applied.
    ///
    /// This allows you to pass data from the joining world to the background world, or even cache the [`WorldSwapApp`]
    /// and resume it later with [`SwapCommand::Fork`] or [`SwapCommand::Pass`].
    ///
    /// Note that time in the world in a [`WorldSwapApp`] passed to [`SwapEndRecoveryFn`] will *not* be paused unless
    /// you manually pause it. The `freeze_time` option in [`BackgroundTickRate::Never`] only applies to worlds in
    /// the background.
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
        background_tick_rate: BackgroundTickRate::Never{
            freeze_time: true,
        },
        swap_join_recovery: None,
        abort_on_background_exit: false,
    }
}

impl Plugin for WorldSwapPlugin {
    fn build(&self, app: &mut App) {
        // Require app uses the `Main` schedule, in order to ensure consistency between the initial app and child apps.
        if app.main_schedule_label != Main.into() {
            panic!("failed adding WorldSwapPlugin, app's main_schedule_label is not Main");
        }

        let (sender, receiver) = crossbeam::channel::unbounded();

        let mut worldswap_subapp = App::empty();
        worldswap_subapp
            .insert_resource(self.clone())
            .insert_resource(SwapCommandSender(sender.clone()))
            .insert_resource(SwapCommandReceiver(receiver))
            .insert_resource(BackgroundApp{ app: None })
            .insert_resource(WorldSwapSubAppState::Running);

        //worldswap_subapp.init_schedule(Main); //todo: is this necessary?
        app.insert_sub_app(WorldSwapSubApp, SubApp::new(worldswap_subapp, world_swap_extract));

        // Set up the original App's world as a world-swap child.
        app.add_plugins(ChildCorePlugin)
            .insert_resource(SwapCommandSender(sender));
    }

    fn cleanup(&self, app: &mut App) {
        // Panic if bevy/bevy_render feature is enabled but render subapps haven't been consolidated.
        if app.get_sub_app(RenderApp).is_ok() && app.get_sub_app(RenderExtractApp).is_ok() {
            panic!("failed removing render subapp, WorldSwapPlugin must be added after DefaultPlugins");
        }

        // Add the current world as the foreground app in the world-swap subapp.
        let maybe_render_app = app.remove_sub_app(RenderApp).or_else(|| app.remove_sub_app(RenderExtractApp));
        let worldswap_subapp = app.sub_app_mut(WorldSwapSubApp);

        worldswap_subapp.insert_resource(ForegroundApp{
            render_app: maybe_render_app,
            // The initial app gets the default background tick rate.
            background_tick_rate: Some(self.background_tick_rate),
        });
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Plugin group for setting up Bevy plugins in a child world.
///
/// This is a wrapper around Bevy's [`DefaultPlugins`], so you can edit the plugin group in the same way.
/// The [`RenderPlugin`] and [`WinitPlugin`] should **not** be edited.
///
/// Don't use this for setting up your initial app. Use [`WorldSwapPlugin`] and [`DefaultPlugins`] instead.
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
        DefaultPlugins::build()
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
