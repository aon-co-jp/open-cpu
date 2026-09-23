//! Android JNIエクスポート(2026-09-23新設、ユーザー指示「APKビルド時に
//! jniLibsとして同梱して下さい〈NNAPIプローブと同じ経路〉」への対応)。
//!
//! `open-english`のAndroidアプリ(`mobile/android/app/src/main/cpp/
//! nnapi_probe.c`と同じ「ネイティブ共有ライブラリをJNI経由で呼ぶ」設計)が
//! `System.loadLibrary("opencpu_jni")`でロードし、`OpenCpuProbe.kt`(新設)
//! から`inventoryJson()`として呼び出す想定。
//!
//! **設計上の理由(NNAPIプローブとの違い)**: NNAPIプローブはアプリの
//! `src/main/cpp/`にC言語ソースとして置かれ、アプリ自身のGradle/CMake
//! ビルドでコンパイルされる。対してopen-cpuはRust製の独立リポジトリの
//! ままにしたい(このエコシステム共通ルール「各プロジェクトの実装を
//! 他リポジトリへ複製しない」)ため、こちらは`cargo ndk`でopen-cpu
//! リポジトリ側で事前ビルドした`.so`を、ビルド済み成果物として
//! アプリの`jniLibs/arm64-v8a/`へ配置する(プリビルドshared library方式)。
//! CIでの自動化は次回以降の課題(今回は手動配置)。
//!
//! **正直な開示**: 現時点でエクスポートしているのは`inventory()`の
//! JSON文字列化のみ(`bench`が行う実ベンチマーク実行はまだJNI経由では
//! 呼べない)。

use jni::objects::{JClass, JString};
use jni::sys::jstring;
use jni::JNIEnv;

/// `tokyo.runo.openenglish.OpenCpuProbe.inventoryJson()`(Kotlin側)から
/// 呼ばれるJNIエントリポイント。`crate::inventory().to_json()`をそのまま
/// Java文字列として返す。JSON生成自体が失敗することは無い設計
/// (`to_json`は`serde_json`のシリアライズ失敗を想定していない単純な
/// 手書きJSON構築のため)なので、この関数はエラー処理を持たない。
#[no_mangle]
pub extern "system" fn Java_tokyo_runo_openenglish_OpenCpuProbe_inventoryJson<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    let json = crate::inventory().to_json();
    let output: JString = match env.new_string(json) {
        Ok(s) => s,
        Err(_) => match env.new_string("{\"error\":\"failed to allocate JNI string\"}") {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        },
    };
    output.into_raw()
}
