use ideck_sd_host::ManifestError;

/// Convert internal errors into short, user-facing messages.
pub fn friendly_error(err: impl std::fmt::Display) -> String {
    friendly_message(&err.to_string())
}

pub fn friendly_anyhow(err: anyhow::Error) -> String {
    if let Some(manifest) = err.downcast_ref::<ManifestError>() {
        return friendly_manifest_err(manifest);
    }
    friendly_message(&err.to_string())
}

pub fn friendly_message(raw: &str) -> String {
    let lower = raw.to_lowercase();

    if lower.contains("exclusive access")
        || lower.contains("already open")
        || lower.contains("0xe00002c5")
        || lower.contains("hid_open_path")
    {
        return "Stream Deck は別のアプリ（Elgato Stream Deck など）が使用中です。\
                そのアプリを終了してから、デバイスの抜き差し後に再接続してください。"
            .into();
    }

    if lower.contains("device already connected") {
        return "この Stream Deck はすでに接続済みです。".into();
    }

    if lower.contains("unknown device kind") {
        return format!("未対応の Stream Deck モデルです: {raw}");
    }

    if let Some(rest) = raw.strip_prefix("manifest missing required field: ") {
        return format!(
            "manifest.json に必須フィールド `{rest}` がありません。プラグインバンドルが壊れている可能性があります。"
        );
    }

    if lower.contains("manifest not found") {
        return "manifest.json が見つかりません。`.sdPlugin` バンドルの構成を確認してください。".into();
    }

    if lower.contains("missing field") && (lower.contains("uuid") || lower.contains("uuid`")) {
        return "manifest.json の形式が正しくありません。\
                各アクションに Elgato 形式の `UUID` フィールドがあるか確認してください。"
            .into();
    }

    if lower.contains("json:") || lower.contains("serde") {
        if let Some(idx) = raw.find("json:") {
            return friendly_manifest_json_error(raw.get(idx + 5..).unwrap_or(raw));
        }
        return friendly_manifest_json_error(raw);
    }

    if let Some(err) = raw.strip_prefix("manifest: json: ") {
        return friendly_manifest_json_error(err);
    }

    if lower.contains("unsupported code path") {
        return "このプラグインの実行ファイル形式にはまだ対応していません。".into();
    }

    if lower.contains("no such file") || lower.contains("not found") && lower.contains("node") {
        return "Node.js が見つかりません。Node をインストールするか PATH を確認してください。".into();
    }

    raw.to_string()
}

fn friendly_manifest_json_error(detail: &str) -> String {
    let trimmed = detail.trim();
    if trimmed.contains("UUID") || trimmed.contains("Uuid") {
        return "manifest.json を読み込めません。`UUID` / `Name` / `Version` / `CodePath` など \
                Elgato 形式のフィールド名になっているか確認してください。"
            .into();
    }
    format!(
        "manifest.json を読み込めません: {trimmed}"
    )
}

pub fn friendly_manifest_err(err: &ManifestError) -> String {
    match err {
        ManifestError::NotFound => friendly_message("manifest not found"),
        ManifestError::MissingField(field) => {
            friendly_message(&format!("manifest missing required field: {field}"))
        }
        ManifestError::Json(e) => friendly_manifest_json_error(&e.to_string()),
        ManifestError::Io(e) => friendly_error(e),
    }
}
