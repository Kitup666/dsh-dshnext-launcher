// 嵌入 Windows 资源：应用图标（assets/icons/app.ico，蓝鲸）。
// 没有它任务栏 / Alt+Tab / 资源管理器里都是默认的灰图标。
// 注意 winresource 会写默认 manifest，这里显式给一个不含 DPI 声明的最小
// manifest——DPI 感知由 winit 运行时自己设（per-monitor v2），别在这里抢。
fn main() {
    // 显式声明依赖，别用「盯整个包」的默认行为：默认下改任何文件（包括
    // HANDOFF.md 这类文档）都触发 build.rs 重跑 + 全量重编。exe 图标在这里
    // 烧进资源段，assets 必须显式在跟踪范围内。
    println!("cargo:rerun-if-changed=assets");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/app.ico");
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
