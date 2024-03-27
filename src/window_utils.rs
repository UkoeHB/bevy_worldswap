use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use bevy::window::{WindowBackendScaleFactorChanged, WindowScaleFactorChanged, WindowThemeChanged};
use bevy::winit::WinitWindows;

//-------------------------------------------------------------------------------------------------------------------

pub(crate) fn map_winit_window_entities(
    windows_a: &WinitWindows,
    windows_b: &WinitWindows,
    entity_a: Entity,
) -> Option<Entity>
{
    let window_id = windows_a.entity_to_winit.get(&entity_a)?;
    windows_b.winit_to_entity().get(window_id)
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource, Default)]
pub(crate) struct WindowEventCache
{
    backend_scale_factor_events: EntityHashMap<WindowBackendScaleFactorChanged>,
    scale_factor_events: EntityHashMap<WindowScaleFactorChanged>,
    theme_events: EntityHashMap<WindowThemeChanged>,
}

impl WindowEventCache
{
    pub(crate) fn remove(&mut self, entity: Entity)
    {
        self.backend_scale_factor_events.remove(&entity);
        self.scale_factor_events.remove(&entity);
        self.theme_events.remove(&entity);
    }

    pub(crate) fn insert_backend_scale_factor_event(&mut self, event: WindowBackendScaleFactorChanged)
    {
        self.backend_scale_factor_events.insert(event.window, event);
    }

    pub(crate) fn insert_scale_factor_event(&mut self, event: WindowScaleFactorChanged)
    {
        self.scale_factor_events.insert(event.window, event);
    }

    pub(crate) fn insert_theme_event(&mut self, event: WindowThemeChanged)
    {
        self.theme_events.insert(event.window, event);
    }

    pub(crate) fn dispatch(
        &mut self,
        main_windows: &WinitWindows,
        new_windows: &WinitWindows,
        new_world: &mut World,
    )
    {
        for (entity, mut event) in self.backend_scale_factor_events.drain()
        {
            // Drop events that don't have matching entities.
            let Some(new_world_entity) = map_winit_window_entities(main_windows, new_windows, entity)
            else
            {
                continue;
            };

            // Map the event's window.
            event.window = new_world_entity;

            // Forward to the new world.
            new_world.send_event(event);
            new_world.send_event(WinitEvent::WindowBackendScaleFactorChanged(event));
        }

        for (entity, mut event) in self.scale_factor_events.drain()
        {
            // Drop events that don't have matching entities.
            let Some(new_world_entity) = map_winit_window_entities(main_windows, new_windows, entity)
            else
            {
                continue;
            };

            // Map the event's window.
            event.window = new_world_entity;

            // Forward to the new world.
            new_world.send_event(event);
            new_world.send_event(WinitEvent::WindowScaleFactorChanged(event));
        }

        for (entity, mut event) in self.theme_events.drain()
        {
            // Drop events that don't have matching entities.
            let Some(new_world_entity) = map_winit_window_entities(main_windows, new_windows, entity)
            else
            {
                continue;
            };

            // Map the event's window.
            event.window = new_world_entity;

            // Forward to the new world.
            new_world.send_event(event);
            new_world.send_event(WinitEvent::WindowThemeChanged(event));
        }
    }
}

//-------------------------------------------------------------------------------------------------------------------
