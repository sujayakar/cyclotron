
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
