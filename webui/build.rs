// 桌面窗口宿主的 Windows 资源：鲸鱼图标（assets/icons/webui.ico）。字符串版
// 信息（FileDescription/ProductName=DeepseekHarness 等）在 Cargo.toml 的
// [package.metadata.winresource] 里——winresource 自动读取。dock/任务管理器/
// 资源管理器显示的就是这套内嵌信息，这正是宿主必须独立成 bin 的原因（硬链接
// 换皮的 exe 与启动器同字节，图标/描述永远甩不开）。
// manifest 与启动器同款（asInvoker、无 DPI 声明——DPI 感知由窗口自己设）。
fn main() {
    println!("cargo:rerun-if-changed=../assets/icons/webui.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/icons/webui.ico");
        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 / 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#,
        );
        res.compile().expect("embed windows resources");
    }
}
