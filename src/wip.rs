

//-------------------------------------------------------------------------------------------------------------------

/*
Implementation plan: Swap between worlds that run in the same update loop (and render to the same window(s)).

- Bevy changes required
    - subapp execution order should equal registration order (without this, it's possible to get 1-frame hiccups when
    swapping worlds)
    - pass the parent App to a subapp on extract, rather than its World (this allows removing/adding other subapps)
    - update AssetPlugin so it won't add an AssetServer if one already exists
    - In the winit runner, there are SystemStates cached outside the running App. These are initialized with the original
    world, so subsequent accesses will be invalid.
        - Add a check with .matches_world(). If not matching, then reinitialize the query states.

- Bevy changes nice-to-have
    - make AudioPlaySet public
    - add system set to bevy_gilrs that can be disabled
    - add system set to bevy_text for update_text2d_layout
    - add system sets to bevy_ui for update_target_camera_system, update_clipping_system,
    compute_slices_on_asset_event (need to disable?), compute_slices_on_image_change (need to disable?)

- caveats
    - Background apps and rendering apps will log to the same output stream.
    - SubApps in secondary apps you construct will be ignored (other than RenderApp/RenderExtractApp).

- questions
    - Is it necessary to clear all windows when swapping between worlds? For example, you may want your menu to stay
    in one window, and your game to pop up in a new window. The menu window should go black, not display the last rendered
    frame from the menu world.

- WindowpassPlugin
    - background_tick_rate: controls how the primary world updates while in the background.
        - Never (default)
        - EveryTick
        - TickRate
        - Custom(callback)
            - Callback takes a ref to the world and returns true if it should update.
    - swap_end_recovery: Option<SwapEndRecovery>,
        - Callback that takes a secondary world by value, and the primary world by &mut.
    - abort_on_primary_exit
        - if true, then force-quit when the primary app exits (does nothing on BackgroundTickRate::Never)

- Examples
    - Main menu -> start game (displays timer) -> return to main menu -> return to game -> exit game.
    - Main menu -> start game (displays region #) -> move to new region (displays region #) -> move back to starting region.


struct WorldSwapApp {
    world: Option<World>,
    time_channel: Option<Res<TimeReceiver>>,
    render_app: Option<RenderApp>,
    extract_app: Option<RenderExtractApp>,
}

Procedure:
- construct new app
    - API
        - requirements
            - app MUST use the same main_schedule_label as the main world
            - WorldSwapBevyPlugins: wrapper around bevy's DefaultPlugins, can use plugin builder to edit
                - must clone AssetServer and pass it to the WorldSwapBevyPlugins
                    - this will be inserted to the new world so assets are visible cross-world
                - RenderPlugin must use RenderCreation::Manual
                    - populate with resources from current world
                        - clone: RenderDevices, RenderQueue, RenderAdapterInfo, RenderAdapter, RenderInstance
                - WindowPlugin must set WindowPlugin::primary_window = None
                - if AppExit is detected, intercept it and convert it into a 'swap end' command in SwapEndSet Last.
                - disable WinitPlugin and use WorldSwapWinitPlugin
                    - init nonsend: WinitWindows
                    - add event: WinitEvent
                    - add systems: changed_windows
                    - add plugin: AccessKitPlugin
        - interface
            - Any app can 'swap start', which will start a new WorldSwapApp and if the current world is primary it will go
            into the background.
            - Primary apps can 'swap pass' to another app, which designates the new app as primary and drops the old primary.
                - Equivalent to 'swap end' for non-primary apps. Can use WorldSwapInfo::is_primary to check.
            - Apps can send a 'swap end' command to suspend the app and call the swap_end_recovery callback (or exit).
                - If you want swap_end_recovery to do something different if the app is suspended or exited, then add that
                info to the app itself (e.g. as a State or resource with flag).
    - call App::finish() -> App::clean() before removing the world and subapps
    - WorldSwapApp{ world, subapps }
- send new app in channel to subapp
- subapp
    - update cached world (note: do this first, we only want to update cached if it was cached during the last App update)
        - run Main schedule manually
        - if cached world sends AppExit, then discard it
    - listen for 'swap-start' command
        - swap running world with new world
            - move resources from running world to new world
                - TimeReceiver (move into WindowSwapApp struct so the background app doesn't try to receive time)
                - EventLoopProxy (clone)
                - Update window entities in new world, using WinitWindows maps from each app (use window ids for cross-map)
                    - Must MOVE winit::window::Window values between WinitWindows instances, since those actually store
                    the winit window instance (bevy Window is just a config transport).
                    - Despawn cached new-world entities if their matching running-world entities don't exist.
                    - Spawn new-world entities if running-world entities don't exist in the map.
                        - Manually add these to WinitWindows in new world, since we don't want to use create_windows(), which
                        actually spawns new winit windows.
                        - Manually add RawHandleWrapper and CachedWindow components
                        - Add PrimaryWindow component if necessary
                        - send WindowCreated event
                    - Clone Window components into existing entities.
                    - Update AccessKitAdapters, WinitActionHandlers
                - Issue cached window events in the new world (need to map window entities)
                    - WindowBackendScaleFactorChanged (most recent for a specific window)
                    - WindowScaleFactorChanged (most recent for a specific window)
                    - WindowThemeChanged (most recent for a specific window)
            - swap World instances
            - swap RenderApp/RenderExtractApp SubApp instances
            - save swapped-out world's WorldSwapApp for swapping back in
        - if no world cached
            - disable the running world
                - set flag in WorldSwapInfo resource
            - cache it
    - listen for 'swap-end' command
        - ignore if current running world's ID != swap-end id (this way you can windowpass between secondary worlds without
        needing to deal with running the swap-end recovery callback *and then* running another callback to pass resources
        from the main world to the new running world; you can do a chained sequence that goes 'swap-start' -> 'swap-start' ->
        'swap-end', ending back in the main world)
        - exit if there is no cached world (implies cached world shut down)
        - swap running world with cached world
            - move resources from running world to cached world
                - TimeReceiver (move back into cached world)
                - Update window entities in cached world
                - Issue cached window events in the cached world
            - re-enable cached world
                - unset flag in WorldSwapInfo resource
            - return the cached RenderApp/RenderExtractApp SubApp, discard the removed one
            - run 'swap-end recovery' callback on the two worlds
                - this moves the running world into the callback, so you can trivially 'pause' the running world and then
                restart it with another 'swap-start'

- TODO: Bevy system sets to add run condition for WorldSwapInfo::is_in_background()
    - audio
        - AudioPlaySet
    - GilRs
        - TODO
    - gizmos
        - GizmoRenderSystem
    - input
        - InputSystem
    - PBR
        - SimulationLightSystems (multiple system sets)
    - render
        - CameraUpdateSystem
        - VisibilitySystems (multiple system sets)
    - text
        - update_text2d_layout: TODO
    - UI
        - UiSystem (multiple system sets)
        - AmbiguousWithTextSystem
        - AmbiguousWithUpdateText2DLayout
        - others: TODO
*/


//-------------------------------------------------------------------------------------------------------------------


