use super::component::ComponentStore;
use super::entity::EntityId;
use super::World;

pub struct Query1<'w, T: Clone + 'static> {
    store: Option<&'w ComponentStore<T>>,
    pub(super) world: &'w World,
    index: usize,
    len: usize,
}

impl<'w, T: Clone + 'static> Query1<'w, T> {
    pub fn new(world: &'w World) -> Self {
        let store = world.get_store::<T>();
        let len = store.map(|s| s.len()).unwrap_or(0);
        Self {
            store,
            world,
            index: 0,
            len,
        }
    }
}

impl<'w, T: Clone + 'static> Iterator for Query1<'w, T> {
    type Item = (EntityId, &'w T);

    fn next(&mut self) -> Option<Self::Item> {
        let store = self.store?;
        let offset = self.world.start_offset();
        while self.index < self.len {
            let idx = self.index;
            self.index += 1;
            if let Some((gen, data)) = store.get_by_index(idx) {
                // Translate local index → global EntityId
                let id = EntityId {
                    index: idx as u32 + offset,
                    generation: *gen,
                };
                if self.world.is_alive(id) {
                    return Some((id, data));
                }
            }
        }
        None
    }
}

pub struct Query2<'w, A: Clone + 'static, B: Clone + 'static> {
    inner: Query1<'w, A>,
    _phantom: std::marker::PhantomData<B>,
}

impl<'w, A: Clone + 'static, B: Clone + 'static> Query2<'w, A, B> {
    pub fn new(world: &'w World) -> Self {
        Self {
            inner: Query1::new(world),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<'w, A: Clone + 'static, B: Clone + 'static> Iterator for Query2<'w, A, B> {
    type Item = (EntityId, &'w A, &'w B);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (id, a) = self.inner.next()?;
            if let Some(b) = self.inner.world.get::<B>(id) {
                return Some((id, a, b));
            }
        }
    }
}
