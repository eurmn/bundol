import { checkUpdate } from "@tauri-apps/api/updater";
import { WebviewWindow } from "@tauri-apps/api/window";

export function checkForUpdates() {
  let c = async () => {
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

        w.listen("user-interacted", () => {
          clearInterval(i);
        });
      }
    } catch (error) {
      console.error(error);
    }
  };

  c();
  let i = setInterval(c, 1000 * 60 * 20);
}
