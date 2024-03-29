use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};

use bevy::{ecs::storage::SparseSetIndex, prelude::*, render::{Render, RenderSet}};

//-------------------------------------------------------------------------------------------------------------------

fn set_render_worker(worker: Res<RenderWorker>)
{
    worker.set();
}

//-------------------------------------------------------------------------------------------------------------------

fn unset_render_worker(worker: Res<RenderWorker>)
{
    worker.unset();
}


//-------------------------------------------------------------------------------------------------------------------

fn _is_target_worker(worker: Res<RenderWorker>) -> bool
{
    worker._matches_target()
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Deref, Eq, PartialEq)]
pub struct RenderWorkerId(pub(crate) usize);

impl Default for RenderWorkerId
{
    fn default() -> Self
    {
        Self(usize::MAX)
    }
}

impl From<&World> for RenderWorkerId
{
    fn from(world: &World) -> Self
    {
        Self(world.id().sparse_set_index())
    }
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource, Clone)]
pub struct RenderWorkerTarget
{
    worker: Arc<AtomicUsize>,
}

impl RenderWorkerTarget
{
    pub(crate) fn new() -> Self
    {
        Self{ worker: Arc::new(AtomicUsize::new(usize::MAX)) }
    }

    pub fn id(&self) -> RenderWorkerId
    {
        RenderWorkerId(self.worker.load(Ordering::Relaxed))
    }

    pub(crate) fn set(&self, id: RenderWorkerId)
    {
        self.worker.store(*id, Ordering::Relaxed);
    }

    pub(crate) fn unset(&self)
    {
        self.worker.store(usize::MAX, Ordering::Relaxed);
    }
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Resource, Clone)]
pub(crate) struct RenderWorker
{
    pub(crate) id: RenderWorkerId,
    pub(crate) target: RenderWorkerTarget,
}

impl RenderWorker
{
    pub(crate) fn _matches_target(&self) -> bool
    {
        self.id == self.target.id()
    }

    pub(crate) fn set(&self)
    {
        self.target.set(self.id);
    }

    pub(crate) fn unset(&self)
    {
        self.target.unset();
    }
}

//-------------------------------------------------------------------------------------------------------------------

/// Plugin to add to RenderApps.
///
/// Disables `RenderSet::Render` if the render app's world is not in the foreground.
/// We only disable the render step so the rest of the rendering data flow won't be disrupted.
pub(crate) struct RenderWorkerPlugin
{
    pub(crate) worker: RenderWorker,
}

impl Plugin for RenderWorkerPlugin
{
    fn build(&self, app: &mut App)
    {
        app.insert_resource(self.worker.clone())
            //.configure_sets(Render, RenderSet::Render.run_if(is_target_worker));
            .add_systems(ExtractSchedule, set_render_worker)
            .add_systems(Render, unset_render_worker.in_set(RenderSet::Cleanup));
    }
}

//-------------------------------------------------------------------------------------------------------------------
