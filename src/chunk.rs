use bevy::{
    prelude::*,
    utils::{HashMap, HashSet},
};

#[derive(Debug, Default)]
pub struct Chunk {
    /// Server ids present in the chunk.
    occupants: HashSet<u64>,
}

#[derive(Resource, Debug)]
pub struct ChunkManager {
    chunk_size: f32,
    loaded_chunks: HashMap<IVec2, Chunk>,
    locations: HashMap<u64, IVec2>,
    visible_objs: HashMap<u64, HashSet<u64>>,
}

const THREE_BY_THREE: [IVec2; 9] = [
    IVec2::new(-1, -1),
    IVec2::new(0, -1),
    IVec2::new(1, -1),
    IVec2::new(-1, 0),
    IVec2::new(0, 0),
    IVec2::new(1, 0),
    IVec2::new(-1, 1),
    IVec2::new(0, 1),
    IVec2::new(1, 1),
];

impl ChunkManager {
    pub fn new(chunk_size: f32) -> Self {
        Self {
            chunk_size,
            loaded_chunks: HashMap::new(),
            locations: HashMap::new(),
            visible_objs: HashMap::new(),
        }
    }

    pub fn chunk_size(&self) -> f32 {
        self.chunk_size
    }

    pub fn load_chunks_near(&mut self, chunk_pos: IVec2) {
        for dir in THREE_BY_THREE {
            let _ = self
                .loaded_chunks
                .try_insert(chunk_pos + dir, Chunk::default());
        }
    }

    pub fn get_location(&self, server_id: u64) -> Option<&IVec2> {
        self.locations.get(&server_id)
    }

    /// Removes all chunks where outside of observers. Returns entites that have no
    /// observers.
    pub fn purge_chunks(&mut self, observers: &HashSet<u64>) -> Vec<u64> {
        let loaded = observers
            .iter()
            .filter_map(|o| {
                let pos = self.locations.get(o)?;
                Some(self.get_chunks_near(*pos).into_iter().map(|v| (v.0)))
            })
            .flatten()
            .collect::<HashSet<IVec2>>();

        let mut removed = Vec::new();
        self.loaded_chunks.retain(|pos, chunk| {
            let keep = loaded.contains(pos);
            if !keep {
                // Linker segfaults when this is std::mem::take... :|
                let occupants = chunk.occupants.clone();
                removed.extend(occupants.into_iter());
            }
            keep
        });

        for remove in removed.iter() {
            self.locations.remove(remove);
        }

        removed
    }

    fn remove_chunk_info(&mut self, server_id: u64) {
        if let Some(curr_chunk) = self.locations.get(&server_id).map(|v| *v) {
            let curr_chunk = self
                .loaded_chunks
                .get_mut(&curr_chunk)
                .expect("chunk must exist");
            curr_chunk.occupants.remove(&server_id);
            self.locations.remove(&server_id);
        }
    }

    pub fn set_chunk_location(&mut self, server_id: u64, chunk_pos: IVec2) {
        self.remove_chunk_info(server_id);

        self.locations.insert(server_id, chunk_pos);
        let next_chunk = self
            .loaded_chunks
            .get_mut(&chunk_pos)
            .expect("chunk must exist");
        next_chunk.occupants.insert(server_id);
    }

    pub fn despawn(&mut self, server_id: u64) {
        self.remove_chunk_info(server_id);

        self.visible_objs.remove(&server_id);
        for visible in self.visible_objs.values_mut() {
            visible.remove(&server_id);
        }
    }

    pub fn get_chunks_near(&self, chunk_pos: IVec2) -> Vec<(IVec2, &Chunk)> {
        THREE_BY_THREE
            .into_iter()
            .map(|v| v + chunk_pos)
            .filter_map(|pos| self.loaded_chunks.get(&pos).map(|c| (pos, c)))
            .collect()
    }

    fn get_visible(&self, pos: IVec2) -> HashSet<u64> {
        let mut curr = HashSet::<u64>::new();
        for (_, c) in self.get_chunks_near(pos) {
            curr.extend(c.occupants.iter());
        }
        curr
    }

    fn get_occupant_diff(&self, server_id: u64) -> (Option<HashSet<u64>>, HashSet<u64>) {
        let Some(chunk) = self.locations.get(&server_id) else {
            return (Some(HashSet::default()), HashSet::default());
        };

        let prev = self.visible_objs.get(&server_id);
        let curr = self.get_visible(*chunk);

        (prev.cloned(), curr)
    }

    pub fn get_nearby_spawns(&self, server_id: u64) -> HashSet<u64> {
        let (prev, curr) = self.get_occupant_diff(server_id);
        match prev {
            Some(prev) => curr.difference(&prev).into_iter().map(|v| *v).collect(),
            None => curr,
        }
    }

    pub fn get_nearby_despawns(&self, server_id: u64) -> HashSet<u64> {
        let (prev, curr) = self.get_occupant_diff(server_id);
        match prev {
            Some(prev) => prev.difference(&curr).into_iter().map(|v| *v).collect(),
            None => HashSet::default(),
        }
    }

    pub fn get_nearby_occupants(&self, server_id: u64) -> HashSet<u64> {
        let Some(chunk) = self.locations.get(&server_id) else {
            return HashSet::default();
        };

        self.get_chunks_near(*chunk)
            .into_iter()
            .flat_map(|(_, chunk)| chunk.occupants.iter().map(|v| *v).collect::<Vec<u64>>())
            .collect()
    }

    pub fn update_visible_objs(&mut self) {
        for (server_id, pos) in self.locations.iter() {
            let next = self.get_visible(*pos);
            self.visible_objs.insert(*server_id, next);
        }
    }

    pub fn chunks(&self) -> impl Iterator<Item = (&IVec2, &Chunk)> {
        self.loaded_chunks.iter()
    }
}

pub fn world_pos_to_chunk_pos(chunk_size: f32, world_pos: Vec2) -> IVec2 {
    (world_pos / chunk_size).floor().as_ivec2()
}

pub fn transform_to_chunk_pos(chunk_size: f32, transform: Transform) -> IVec2 {
    world_pos_to_chunk_pos(chunk_size, transform.translation.truncate())
}
