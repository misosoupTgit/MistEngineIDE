/// 当たり判定システム
/// AABB（粗い判定）→ 分割木（精密判定）

#[derive(Debug, Clone, Copy)]
pub struct AABB {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl AABB {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        AABB { x, y, width, height }
    }

    pub fn from_circle(cx: f32, cy: f32, radius: f32) -> Self {
        AABB {
            x: cx - radius,
            y: cy - radius,
            width: radius * 2.0,
            height: radius * 2.0,
        }
    }

    pub fn intersects(&self, other: &AABB) -> bool {
        self.x < other.x + other.width &&
        self.x + self.width > other.x &&
        self.y < other.y + other.height &&
        self.y + self.height > other.y
    }

    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width &&
        py >= self.y && py <= self.y + self.height
    }
}

#[derive(Debug, Clone)]
pub struct Collider {
    pub id: u64,
    pub aabb: AABB,
    pub pixel_perfect: bool,
    pub active: bool,
}

impl Collider {
    pub fn new(id: u64, aabb: AABB) -> Self {
        Collider { id, aabb, pixel_perfect: false, active: true }
    }

    pub fn with_pixel_perfect(mut self) -> Self {
        self.pixel_perfect = true;
        self
    }

    /// 他のコライダーと衝突しているか
    pub fn collides_with(&self, other: &Collider) -> bool {
        if !self.active || !other.active { return false; }
        self.aabb.intersects(&other.aabb)
        // TODO: pixel_perfect の場合は追加でピクセル判定
    }
}

/// 簡易分割木（Quadtree）
pub struct Quadtree {
    bounds: AABB,
    max_objects: usize,
    max_depth: usize,
    objects: Vec<Collider>,
    children: Option<Box<[Quadtree; 4]>>,
    depth: usize,
}

impl Quadtree {
    pub fn new(bounds: AABB, max_objects: usize, max_depth: usize) -> Self {
        Quadtree {
            bounds,
            max_objects,
            max_depth,
            objects: Vec::new(),
            children: None,
            depth: 0,
        }
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.children = None;
    }

    pub fn insert(&mut self, collider: Collider) {
        if !self.bounds.intersects(&collider.aabb) { return; }
        if self.objects.len() < self.max_objects || self.depth >= self.max_depth {
            self.objects.push(collider);
            return;
        }
        if self.children.is_none() {
            self.subdivide();
        }
        if let Some(children) = &mut self.children {
            for child in children.iter_mut() {
                child.insert(collider.clone());
            }
        }
    }

    fn subdivide(&mut self) {
        let hw = self.bounds.width / 2.0;
        let hh = self.bounds.height / 2.0;
        let x = self.bounds.x;
        let y = self.bounds.y;
        let d = self.depth + 1;
        self.children = Some(Box::new([
            Self::with_depth(AABB::new(x,      y,      hw, hh), self.max_objects, self.max_depth, d),
            Self::with_depth(AABB::new(x + hw, y,      hw, hh), self.max_objects, self.max_depth, d),
            Self::with_depth(AABB::new(x,      y + hh, hw, hh), self.max_objects, self.max_depth, d),
            Self::with_depth(AABB::new(x + hw, y + hh, hw, hh), self.max_objects, self.max_depth, d),
        ]));
    }

    fn with_depth(bounds: AABB, max_objects: usize, max_depth: usize, depth: usize) -> Self {
        Quadtree { bounds, max_objects, max_depth, objects: Vec::new(), children: None, depth }
    }

    pub fn query(&self, area: &AABB) -> Vec<&Collider> {
        let mut result = Vec::new();
        if !self.bounds.intersects(area) { return result; }
        for obj in &self.objects {
            if obj.aabb.intersects(area) {
                result.push(obj);
            }
        }
        if let Some(children) = &self.children {
            for child in children.iter() {
                result.extend(child.query(area));
            }
        }
        result
    }
}

pub struct CollisionWorld {
    pub colliders: Vec<Collider>,
    quadtree: Quadtree,
    next_id: u64,
    world_bounds: AABB,
}

impl CollisionWorld {
    pub fn new(width: f32, height: f32) -> Self {
        CollisionWorld {
            colliders: Vec::new(),
            quadtree: Quadtree::new(AABB::new(0.0, 0.0, width, height), 8, 5),
            next_id: 0,
            world_bounds: AABB::new(0.0, 0.0, width, height),
        }
    }

    pub fn add_collider(&mut self, aabb: AABB) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.colliders.push(Collider::new(id, aabb));
        id
    }

    pub fn update_tree(&mut self) {
        self.quadtree.clear();
        for c in &self.colliders {
            self.quadtree.insert(c.clone());
        }
    }

    pub fn check_collision(&self, id: u64) -> Vec<u64> {
        if let Some(col) = self.colliders.iter().find(|c| c.id == id) {
            let candidates = self.quadtree.query(&col.aabb);
            candidates.iter()
                .filter(|c| c.id != id && col.collides_with(c))
                .map(|c| c.id)
                .collect()
        } else {
            Vec::new()
        }
    }
}
