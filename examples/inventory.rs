//! CPU命令セットの全自動インベントリを表示する(`cargo run --example inventory`)。
//! スマホ(aarch64 Android)では `--target aarch64-linux-android` でビルドして `adb push` して実行する。
fn main() {
    let inv = open_cpu::inventory();
    println!("{}", inv.summary());
    if std::env::args().any(|a| a == "--json") {
        println!("{}", inv.to_json());
    }
}
