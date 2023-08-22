import { checkUpdate } from "@tauri-apps/api/updater";
import { WebviewWindow } from "@tauri-apps/api/window";

export async function checkForUpdates() {
  try {
    const { shouldUpdate } = await checkUpdate();

    if (shouldUpdate) {
      let w = new WebviewWindow("updaterWindow", {
        url: "/updater",
        width: 400,
        height: 250,
        decorations: false,
        resizable: false,
        center: true,
        title: "bundol - Atualização Disponível",
        focus: true,
      });

      w.listen("tauri://error", console.log);
    }
  } catch (error) {
    console.error(error);
  }
}
