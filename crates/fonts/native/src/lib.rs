use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fontdb::{Database, Family, Query, Style, Weight};
use spiders_css::FontQuery;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NativeFontQueryKey {
    pub query: FontQuery,
}

impl From<&FontQuery> for NativeFontQueryKey {
    fn from(query: &FontQuery) -> Self {
        Self { query: query.clone() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedNativeFont {
    pub family_name: String,
    pub source_name: Option<String>,
    pub data: Arc<Vec<u8>>,
}

pub trait NativeFontResolver: std::fmt::Debug + Send + Sync {
    fn resolve(&self, query: &FontQuery) -> Option<ResolvedNativeFont>;
}

#[derive(Debug, Default)]
pub struct CachedNativeFontResolver<R> {
    inner: R,
    cache: Mutex<HashMap<NativeFontQueryKey, Option<ResolvedNativeFont>>>,
}

impl<R> CachedNativeFontResolver<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, cache: Mutex::new(HashMap::new()) }
    }
}

impl<R> NativeFontResolver for CachedNativeFontResolver<R>
where
    R: NativeFontResolver,
{
    fn resolve(&self, query: &FontQuery) -> Option<ResolvedNativeFont> {
        let key = NativeFontQueryKey::from(query);
        if let Some(cached) =
            self.cache.lock().expect("font cache mutex poisoned").get(&key).cloned()
        {
            return cached;
        }

        let resolved = self.inner.resolve(query);
        self.cache.lock().expect("font cache mutex poisoned").insert(key, resolved.clone());
        resolved
    }
}

#[derive(Debug, Default)]
pub struct NullNativeFontResolver;

impl NativeFontResolver for NullNativeFontResolver {
    fn resolve(&self, _query: &FontQuery) -> Option<ResolvedNativeFont> {
        None
    }
}

#[derive(Debug)]
pub struct FontDbNativeFontResolver {
    db: Database,
    bytes_cache: Mutex<HashMap<fontdb::ID, Option<Arc<Vec<u8>>>>>,
}

impl Default for FontDbNativeFontResolver {
    fn default() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self { db, bytes_cache: Mutex::new(HashMap::new()) }
    }
}

impl NativeFontResolver for FontDbNativeFontResolver {
    fn resolve(&self, query: &FontQuery) -> Option<ResolvedNativeFont> {
        let families = query.families.iter().map(fontdb_family_from_css).collect::<Vec<_>>();
        let fontdb_query = Query {
            families: &families,
            weight: fontdb_weight(query),
            stretch: fontdb::Stretch::Normal,
            style: Style::Normal,
        };
        let id = self.db.query(&fontdb_query)?;
        let data = self.cached_face_bytes(id)?;
        let face = self.db.face(id)?;
        Some(ResolvedNativeFont {
            family_name: face.families.first().map(|family| family.0.clone()).unwrap_or_default(),
            source_name: Some(format!("{:?}", face.source)),
            data,
        })
    }
}

impl FontDbNativeFontResolver {
    fn cached_face_bytes(&self, id: fontdb::ID) -> Option<Arc<Vec<u8>>> {
        if let Some(bytes) =
            self.bytes_cache.lock().expect("font byte cache mutex poisoned").get(&id).cloned()
        {
            return bytes;
        }

        let bytes = self.load_face_bytes(id).map(Arc::new);
        self.bytes_cache.lock().expect("font byte cache mutex poisoned").insert(id, bytes.clone());
        bytes
    }

    fn load_face_bytes(&self, id: fontdb::ID) -> Option<Vec<u8>> {
        let face = self.db.face(id)?;
        match &face.source {
            fontdb::Source::Binary(data) => Some(data.as_ref().as_ref().to_vec()),
            fontdb::Source::File(path) => std::fs::read(path).ok(),
            fontdb::Source::SharedFile(path, _) => std::fs::read(path).ok(),
        }
    }
}

fn fontdb_family_from_css(family: &spiders_css::FontFamilyName) -> Family<'static> {
    match family {
        spiders_css::FontFamilyName::Named(name) => {
            Family::Name(Box::leak(name.clone().into_boxed_str()))
        }
        spiders_css::FontFamilyName::Serif => Family::Serif,
        spiders_css::FontFamilyName::SansSerif => Family::SansSerif,
        spiders_css::FontFamilyName::Monospace => Family::Monospace,
        spiders_css::FontFamilyName::Cursive => Family::Cursive,
        spiders_css::FontFamilyName::Fantasy => Family::Fantasy,
        spiders_css::FontFamilyName::SystemUi => Family::SansSerif,
    }
}

fn fontdb_weight(query: &FontQuery) -> Weight {
    match query.weight {
        spiders_css::FontWeightValue::Normal => Weight::NORMAL,
        spiders_css::FontWeightValue::Bold => Weight::BOLD,
    }
}
