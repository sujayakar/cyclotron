pub trait Ident: Copy+Clone {
    fn to_usize(self) -> usize;
    fn from_usize(v: usize) -> Self;
}

pub struct VecDefaultMap<Id: Ident, V: Default> {
    inner: Vec<V>,
    _phantom: std::marker::PhantomData<Id>,
}

impl<Id: Ident, V: Default> VecDefaultMap<Id, V> {
    pub fn new() -> Self {
        VecDefaultMap {
            inner: Vec::new(),
            _phantom: Default::default(),
        }
    }

    pub fn entry(&mut self, id: Id) -> &mut V {
        let index = id.to_usize();
        while self.inner.len() <= index {
            self.inner.push(Default::default());
        }
        &mut self.inner[index]
    }

    pub fn get(&self, id: Id) -> &V {
        let index = id.to_usize();
        &self.inner[index]
    }

    pub fn into_vec(self) -> Vec<V> {
        self.inner
    }
}

pub struct SliceDefaultMapIter<'a, Id: Ident, V: Default> {
    inner: std::iter::Enumerate<std::slice::Iter<'a, V>>,
    _phantom: std::marker::PhantomData<Id>,
}

impl<'a, Id: Ident, V: Default> IntoIterator for &'a VecDefaultMap<Id, V> {
    type Item = (Id, &'a V);
    type IntoIter = SliceDefaultMapIter<'a, Id, V>;

    fn into_iter(self) -> Self::IntoIter {
        SliceDefaultMapIter {
            inner: self.inner.iter().enumerate(),
            _phantom: Default::default(),
        }
    }
}

impl<'a, Id: Ident, V: Default> Iterator for SliceDefaultMapIter<'a, Id, V> {
    type Item = (Id, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(i, v)| (Id::from_usize(i), v))
    }
}

fn hue_to_p(p: f32, q: f32, mut t: f32) -> f32 {
    if t <0.00 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0/6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0/2.0 {
        return q;
    }
    if t < 2.0/3.0 {
        return p + (q - p) * (2.0/3.0 - t) * 6.0;
    }
    p
}

pub fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };

        let p = 2.0 * l - q;

        (
            hue_to_p(p, q, h + 1.0/3.0),
            hue_to_p(p, q, h),
            hue_to_p(p, q, h - 1.0/3.0),
        )
    }
}
