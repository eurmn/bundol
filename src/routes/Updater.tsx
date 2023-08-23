import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { relaunch } from "@tauri-apps/api/process";
import { checkUpdate, installUpdate } from "@tauri-apps/api/updater";
import { UserAttentionType, appWindow } from "@tauri-apps/api/window";
import { Show, createSignal } from "solid-js";

export function Updater() {
  const [progress, setProgress] = createSignal(0);
  const [newVersion, setNewVersion] = createSignal<string>();
  const [currentVersion, setCurrentVersion] = createSignal<string>();
  const [updating, setUpdating] = createSignal(false);

  checkUpdate().then(({ manifest }) => {
    if (manifest?.version) setNewVersion(manifest?.version);
  });

  getVersion().then((v) => setCurrentVersion(v));
  appWindow.requestUserAttention(UserAttentionType.Critical);

  async function ignoreUpdate() {
    await appWindow.emit("user-interacted");
    await appWindow.close();
  }

  async function update() {
    setUpdating(true);

    let unlisten = await listen(
      "tauri://update-download-progress",
      (event: any) => {
        setProgress(
          progress() +
            Math.floor(
              (event.payload.chunkLength * 100) / event.payload.contentLength
            )
        );
      }
    );

    try {
      await installUpdate();
      unlisten();
      await relaunch();
    } catch (e) {
      console.log(e);
    }
  }

  return (
    <div class="flex flex-col w-full h-full bg-dark-950 text-truegray-2 pt-7 p-5">
      <div class="h-7 absolute top-0 left-0 w-full" data-tauri-drag-region />
      <div class="text-xl text-center font-bold flex gap-3 items-center justify-center">
        <div class="i-mdi-download" />
        <span>Atualização Disponível</span>
      </div>
      <div class="text-truegray-3 text-sm text-center pt-5">
        Há uma nova atualização disponível para o bundol.
      </div>
      <div class="my-auto text-truegray-3 text-center font-semibold flex gap-2 justify-center items-center">
        <Show when={newVersion() && currentVersion()}>
          <div>v{currentVersion()}</div>
          <div class="i-mdi-arrow-right" />
          <div>v{newVersion()}</div>
        </Show>
      </div>
      <Show when={!updating()}>
        <div class="flex justify-end gap-1 text-sm items-end">
          <div
            onclick={() => ignoreUpdate()}
            class="flex mx-3 my-2 text-truegray-3 cursor-pointer"
          >
            Ignorar
          </div>
          <div
            onclick={() => update()}
            class="bg-indigo-7 rounded-md px-3 py-2 cursor-pointer hover:bg-indigo-8 transition-250"
          >
            Atualizar
          </div>
        </div>
      </Show>
      <div class="transition-100 absolute bottom-0 left-0 h-1 bg-gradient-to-r
        from-green via-green-7to-green w-full"></div>
      <div
        style={`width: ${100 - progress()}%`}
        class="z-10 h-1 bg-dark-950 absolute bottom-0 right-0 transition-30 transition-all"
      ></div>
    </div>
  );
}
