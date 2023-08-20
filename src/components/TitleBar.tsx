import { appWindow } from "@tauri-apps/api/window";

export function TitleBar() {
  return (
    <div
      class="text-truegray-3 bg-dark-7 w-full h-8 flex
            items-center justify-end children:(cursor-pointer p-1) hover:children:bg-dark-3"
      data-tauri-drag-region
    >
      <div class="justify-self-start mr-auto px-3 font-bold font-inter">
        bundol
      </div>
      <div
        onclick={() => appWindow.minimize()}
        class="h-8 w-8 flex justify-center items-center"
      >
        <div class="i-mdi-minus" />
      </div>
      <div class="!cursor-default !hover:bg-dark-7 text-truegray-5 h-8 w-8 flex justify-center items-center">
        <div class="i-mdi-crop-square" />
      </div>
      <div
        onclick={() => appWindow.close()}
        class="h-8 w-8 flex justify-center items-center !hover:bg-red-7"
      >
        <div class="i-mdi-close" />
      </div>
    </div>
  );
}
