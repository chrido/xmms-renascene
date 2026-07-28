#[cfg(feature = "perf-tracing")]
struct TraceGuards {
    _flame: tracing_flame::FlushGuard<std::io::BufWriter<std::fs::File>>,
    _chrome: tracing_chrome::FlushGuard,
}

#[cfg(feature = "perf-tracing")]
thread_local! {
    static TRACE_GUARDS: std::cell::RefCell<Option<TraceGuards>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(feature = "perf-tracing")]
pub fn initialize() -> Result<(), String> {
    use tracing_subscriber::prelude::*;

    if std::env::var("XMMS_PERF_TRACE").ok().as_deref() != Some("1") {
        return Ok(());
    }
    let output = std::env::var_os("XMMS_PERF_TRACE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("target/perf-trace"));
    std::fs::create_dir_all(&output)
        .map_err(|err| format!("failed to create performance trace directory: {err}"))?;
    let (flame_layer, flame_guard) =
        tracing_flame::FlameLayer::with_file(output.join("tracing.folded"))
            .map_err(|err| format!("failed to create tracing flame output: {err}"))?;
    let (chrome_layer, chrome_guard) = tracing_chrome::ChromeLayerBuilder::new()
        .file(output.join("trace.json"))
        .build();
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(flame_layer)
            .with(chrome_layer),
    )
    .map_err(|err| format!("failed to install performance tracing subscriber: {err}"))?;
    TRACE_GUARDS.with(|guards| {
        *guards.borrow_mut() = Some(TraceGuards {
            _flame: flame_guard,
            _chrome: chrome_guard,
        });
    });
    Ok(())
}

#[cfg(not(feature = "perf-tracing"))]
#[inline]
pub fn initialize() -> Result<(), String> {
    Ok(())
}

#[macro_export]
macro_rules! perf_span {
    ($name:literal) => {{
        #[cfg(feature = "perf-tracing")]
        let guard = tracing::info_span!($name).entered();
        #[cfg(not(feature = "perf-tracing"))]
        let guard = ();
        guard
    }};
}
