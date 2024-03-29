use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};

use bevy::{ecs::storage::SparseSetIndex, prelude::*, render::{Render, RenderSet}};

//-------------------------------------------------------------------------------------------------------------------

fn set_render_worker(worker: Res<RenderWorker>)
{
    debug_assert_eq!(worker.target.id(), RenderWorkerId::default());
    worker.set();
}

//-------------------------------------------------------------------------------------------------------------------

fn unset_render_worker(worker: Res<RenderWorker>)
{
    debug_assert_eq!(worker.target.id(), worker.id);
    worker.unset();
}

//-------------------------------------------------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, Deref, Eq, PartialEq)]
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
pub(crate) struct RenderWorkerPlugin
{
    pub(crate) worker: RenderWorker,
}

impl Plugin for RenderWorkerPlugin
{
    fn build(&self, app: &mut App)
    {
        app.insert_resource(self.worker.clone())
            .add_systems(ExtractSchedule, set_render_worker)
            .add_systems(Render, unset_render_worker.in_set(RenderSet::Cleanup));
    }
}

//-------------------------------------------------------------------------------------------------------------------
