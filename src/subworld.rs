use atomic_refcell::AtomicRef;
use smallvec::smallvec;
use std::{any::type_name, marker::PhantomData, ops::Deref};

use crate::{access::*, Context, Error, Result, View};
use hecs::{Component, Entity, Query, QueryBorrow, QueryOne, TypeInfo, World};

/// Type alias for a subworld referencing the world by an atomic ref. Most
/// common for schedules
pub type SubWorld<'a, T> = SubWorldRaw<AtomicRef<'a, World>, T>;
/// Type alias for a subworld referencing the world by a reference
pub type SubWorldRef<'a, T> = SubWorldRaw<&'a World, T>;

pub struct SubWorldRaw<A, T> {
    world: A,
    marker: PhantomData<T>,
}

impl<A, T> SubWorldRaw<A, T> {
    /// Splits the world into a subworld. No borrow checking is performed so may
    /// fail during query unless guarded otherwise.
    pub fn new(world: A) -> Self {
        Self {
            world,
            marker: PhantomData,
        }
    }
}

impl<A: Deref<Target = World>, T: ComponentAccess> SubWorldRaw<A, T> {
    /// Returns true if the subworld has access the borrow of T
    pub fn has<U: IntoAccess>(&self) -> bool {
        T::has::<U>()
    }

    /// Returns true if the world satisfies the whole query
    pub fn has_all<U: Subset>(&self) -> bool {
        U::is_subset::<T>()
    }

    /// Query the subworld.
    /// # Panics
    /// Panics if the query items are not a compatible subset of the subworld.
    pub fn query<'w, Q: Query + Subset>(&'w self) -> QueryBorrow<'w, Q> {
        if !self.has_all::<Q>() {
            panic!("Attempt to execute query on incompatible subworld")
        }

        self.world.query()
    }

    /// Query the subworld.
    /// Fails if the query items are not compatible with the subworld
    pub fn try_query<'w, Q: Query + Subset>(&'w self) -> Result<QueryBorrow<'w, Q>> {
        if !self.has_all::<Q>() {
            return Err(Error::IncompatibleSubworld {
                subworld: T::accesses(),
                query: Q::accesses(),
            });
        } else {
            Ok(self.world.query())
        }
    }

    /// Query the subworld for a single entity.
    /// Wraps the hecs::NoSuchEntity error and provides the entity id
    pub fn query_one<'w, Q: Query + Subset>(&'w self, entity: Entity) -> Result<QueryOne<'w, Q>> {
        if !self.has_all::<Q>() {
            return Err(Error::IncompatibleSubworld {
                subworld: T::accesses(),
                query: Q::accesses(),
            });
        }

        self.world
            .query_one(entity)
            .map_err(|_| Error::NoSuchEntity(entity))
    }

    /// Get a single component from the world.
    ///
    /// If a mutable borrow is desired, use [`Self::query_one`] since the world is
    /// only immutably borrowed.
    ///
    /// Wraps the hecs::NoSuchEntity error and provides the entity id
    pub fn get<C: Component>(&self, entity: Entity) -> Result<hecs::Ref<C>> {
        if !self.has::<&C>() {
            return Err(Error::IncompatibleSubworld {
                subworld: T::accesses(),
                query: smallvec![Access {
                    info: TypeInfo::of::<C>(),
                    exclusive: false,
                    name: type_name::<C>(),
                }],
            });
        }

        self.world.get(entity).map_err(|e| e.into())
    }
}

impl<'a, A, T> View<'a> for SubWorldRaw<A, T>
where
    A: Deref<Target = World>,
    T: ComponentAccess,
{
    type Superset = A;

    fn split(world: Self::Superset) -> Self {
        Self::new(world)
    }
}

impl<'a, T> CellBorrow<'a> for SubWorldRaw<AtomicRef<'a, World>, T> {
    type Target = Self;
    type Cell = World;

    fn borrow(
        cell: &'a atomic_refcell::AtomicRefCell<std::ptr::NonNull<u8>>,
    ) -> Result<Self::Target> {
        let val = cell
            .try_borrow()
            .map_err(|_| Error::Borrow(type_name::<T>()))
            .map(|cell| AtomicRef::map(cell, |val| unsafe { val.cast::<Self::Cell>().as_ref() }))?;

        Ok(Self::new(val))
    }
}

impl<'a, T> From<&'a Context<'a>> for SubWorldRaw<AtomicRef<'a, World>, T> {
    fn from(context: &'a Context) -> Self {
        let borrow = context
            .atomic_ref::<&World>()
            .expect("Failed to borrow world from context")
            .borrow();

        let val = AtomicRef::map(borrow, |val| unsafe { val.cast().as_ref() });

        Self::new(val)
    }
}
