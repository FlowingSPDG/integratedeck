use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use boa_engine::builtins::promise::PromiseState;
use boa_engine::module::{Module, ModuleLoader, Referrer};
use boa_engine::{Context, JsNativeError, JsObject, JsResult, JsString, JsValue, Source, js_string};

use crate::resolver::{is_builtin_shim, normalize_specifier, resolve_specifier, ResolveContext};

pub struct IdeckModuleLoader {
    modules_root: PathBuf,
    shims: HashMap<String, String>,
}

impl IdeckModuleLoader {
    pub fn new(modules_root: PathBuf) -> Self {
        let mut shims = HashMap::new();
        shims.insert(
            "node:path".into(),
            include_str!("../assets/shims/node_path.js").into(),
        );
        shims.insert(
            "node:url".into(),
            include_str!("../assets/shims/node_url.js").into(),
        );
        shims.insert(
            "node:fs".into(),
            include_str!("../assets/shims/node_fs.js").into(),
        );
        shims.insert(
            "node:events".into(),
            include_str!("../assets/shims/node_events.js").into(),
        );
        shims.insert("ws".into(), include_str!("../assets/shims/ws.js").into());

        Self {
            modules_root,
            shims,
        }
    }

    fn parse_source(
        source: &str,
        path: Option<&Path>,
        context: &mut Context,
    ) -> JsResult<Module> {
        let mut bytes = source.as_bytes();
        Module::parse(Source::from_reader(&mut bytes, path), None, context)
    }

    pub fn load_shim_exports(&self, spec: &str, context: &mut Context) -> JsResult<JsValue> {
        let normalized = normalize_specifier(spec);
        let source = self.shims.get(&normalized).ok_or_else(|| {
            JsNativeError::typ().with_message(format!("Cannot find module '{spec}'"))
        })?;
        let module = Self::parse_source(source, Some(Path::new(&normalized)), context)?;
        let promise = module.load_link_evaluate(context);
        context.run_jobs()?;
        match promise.state() {
            PromiseState::Fulfilled(_) => {}
            PromiseState::Rejected(err) => {
                return Err(JsNativeError::typ()
                    .with_message(format!("module '{spec}' failed to evaluate: {err:?}"))
                    .into());
            }
            PromiseState::Pending => {
                return Err(JsNativeError::typ()
                    .with_message(format!("module '{spec}' evaluation pending"))
                    .into());
            }
        }
        let ns = module.namespace(context);
        if let Ok(default_export) = ns.get(js_string!("default"), context) {
            if !default_export.is_undefined() {
                return Ok(default_export);
            }
        }
        Ok(ns.into())
    }
}

impl ModuleLoader for IdeckModuleLoader {
    fn init_import_meta(
        self: Rc<Self>,
        import_meta: &JsObject,
        module: &Module,
        context: &mut Context,
    ) {
        if let Some(path) = module.path() {
            let url = format!(
                "file:///{}",
                path.to_string_lossy().replace('\\', "/")
            );
            let _ = import_meta.create_data_property_or_throw(
                js_string!("url"),
                JsValue::from(JsString::from(url)),
                context,
            );
        }
    }

    async fn load_imported_module(
        self: Rc<Self>,
        referrer: Referrer,
        specifier: JsString,
        context: &RefCell<&mut Context>,
    ) -> JsResult<Module> {
        let spec = normalize_specifier(&specifier.to_std_string_escaped());
        let mut ctx_ref = context.borrow_mut();

        if self.shims.contains_key(&spec) {
            return Self::parse_source(
                self.shims.get(&spec).unwrap(),
                Some(Path::new(&spec)),
                &mut ctx_ref,
            );
        }

        if is_builtin_shim(&spec) {
            return Err(JsNativeError::typ()
                .with_message(format!("missing built-in shim: {spec}"))
                .into());
        }

        let resolve_ctx = ResolveContext {
            modules_root: self.modules_root.clone(),
            referrer_path: referrer.path().map(|p| p.to_path_buf()),
        };

        let path = resolve_specifier(&spec, &resolve_ctx).map_err(|e| {
            JsNativeError::typ().with_message(format!("cannot resolve '{spec}': {e}"))
        })?;

        let source = std::fs::read_to_string(&path).map_err(|e| {
            JsNativeError::typ().with_message(format!("failed to read {}: {e}", path.display()))
        })?;

        Self::parse_source(&source, Some(path.as_path()), &mut ctx_ref)
    }
}
