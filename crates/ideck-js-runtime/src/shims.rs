use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use boa_engine::native_function::NativeFunction;
use boa_engine::property::Attribute;
use boa_engine::{Context, JsArgs, JsObject, JsResult, JsString, JsValue, js_string};

use crate::loader::IdeckModuleLoader;
use crate::resolver::normalize_specifier;

type EventEmitter = Arc<dyn Fn(String, String) + Send + Sync>;

thread_local! {
    static EVENT_EMITTER: RefCell<Option<EventEmitter>> =
        const { RefCell::new(None) };
    static MODULE_LOADER: RefCell<Option<Rc<IdeckModuleLoader>>> = const { RefCell::new(None) };
}

pub fn install_globals(
    context: &mut Context,
    event_emitter: Option<EventEmitter>,
) -> JsResult<()> {
    EVENT_EMITTER.with(|slot| {
        *slot.borrow_mut() = event_emitter;
    });

    let fs = JsObject::with_null_proto();
    fs.set(
        js_string!("existsSync"),
        NativeFunction::from_fn_ptr(|_, args, ctx| {
            let path = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            Ok(JsValue::from(std::path::Path::new(&path).exists()))
        })
        .to_js_function(context.realm()),
        false,
        context,
    )?;

    fs.set(
        js_string!("readFileSync"),
        NativeFunction::from_fn_ptr(|_, args, ctx| {
            let path = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            std::fs::read_to_string(&path)
                .map(|s| JsValue::from(JsString::from(s)))
                .map_err(|e| {
                    boa_engine::JsNativeError::typ()
                        .with_message(format!("readFileSync failed: {e}"))
                        .into()
                })
        })
        .to_js_function(context.realm()),
        false,
        context,
    )?;

    fs.set(
        js_string!("toFileUrl"),
        NativeFunction::from_fn_ptr(|_, args, ctx| {
            let path = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            Ok(JsValue::from(JsString::from(format!(
                "file:///{}",
                path.replace('\\', "/")
            ))))
        })
        .to_js_function(context.realm()),
        false,
        context,
    )?;

    context.register_global_property(
        js_string!("__ideckFs"),
        fs,
        Attribute::all(),
    )?;

    context.register_global_callable(
        js_string!("__ideckEmit"),
        2,
        NativeFunction::from_fn_ptr(|_, args, ctx| {
            let event = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let data_json = args
                .get_or_undefined(1)
                .to_string(ctx)?
                .to_std_string_escaped();
            EVENT_EMITTER.with(|slot| {
                if let Some(emitter) = slot.borrow().as_ref() {
                    emitter(event, data_json);
                }
            });
            Ok(JsValue::undefined())
        }),
    )?;

    Ok(())
}

pub fn install_require(context: &mut Context, loader: Rc<IdeckModuleLoader>) -> JsResult<()> {
    MODULE_LOADER.with(|slot| {
        *slot.borrow_mut() = Some(loader);
    });

    context.register_global_callable(
        js_string!("require"),
        1,
        NativeFunction::from_fn_ptr(|_, args, ctx| {
            let id = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let normalized = normalize_specifier(&id);
            MODULE_LOADER.with(|slot| {
                let loader = slot.borrow();
                let Some(loader) = loader.as_ref() else {
                    return Err(boa_engine::JsNativeError::typ()
                        .with_message("module loader not installed")
                        .into());
                };
                loader.load_shim_exports(&normalized, ctx)
            })
        }),
    )?;

    Ok(())
}
