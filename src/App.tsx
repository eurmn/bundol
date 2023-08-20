import { UnlistenFn, listen } from "@tauri-apps/api/event";
import "highlight.js/styles/atom-one-dark-reasonable.css";
import { For, Show, createEffect, createSignal, onCleanup } from "solid-js";
import "./App.css";
import { TitleBar } from "./components/TitleBar";
import { appWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api";

class LCU {
  public AppPort: number;
  public Password: string;

  constructor(port: number, password: string) {
    this.AppPort = port;
    this.Password = password;
  }

  public async fetch(
    input: RequestInfo | URL,
    init?: RequestInit
  ): Promise<Response> {
    init = init || {};
    init.headers = {
      ...init.headers,
      Authorization: `Basic ${btoa(`riot:${this.Password}`)}`,
    };

    return fetch(input, init);
  }
}

interface ISummonerData {
  summonerName: string;
  summonerIconId: number;
}

const BLIND_PICK_ID = 430;

function App() {
  let listeners: Promise<UnlistenFn>[] = [];

  onCleanup(() => {
    listeners.forEach((l) => l.then((u) => u()));
  });

  const [jsons, setJsons] = createSignal<string[]>([]);
  const [summoners, setSummoners] = createSignal<ISummonerData[]>([]);
  const [gamemode, setGamemode] = createSignal<number>();
  const [online, setOnline] = createSignal<boolean>();
  const [currentSummoner, setCurrentSummoner] = createSignal<string>();
  const [isOnChampSelect, setIsOnChampSelect] = createSignal<boolean>(false);

  /* fetch(
    "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/champion-summary.json"
  ).then((d) => {
    d.json().then((j) => {
      setAllChampions(j.slice(1).map((c: any) => c.id));
    });
  }); */

  invoke<boolean>("is_connected_to_lcu").then(async (r) => {
    setOnline(r);

    if (r) {
      setCurrentSummoner(await invoke<string>("lcu_summoner_name"));

      let lobby = await invoke<string>("get_current_lobby");
      if (!lobby) return;

      setSummoners(
        JSON.parse(lobby).map(
          ({ summonerName, summonerIconId }: ISummonerData) => ({
            summonerName,
            summonerIconId,
          })
        )
      );
    }
  });

  listeners.push(
    appWindow.listen("lcu-connected", () => {
      console.log("LCU connected");
      setOnline(true);
    })
  );

  listeners.push(
    appWindow.listen("lcu-disconnected", () => {
      console.log("LCU disconnected");
      setOnline(false);
      setCurrentSummoner(undefined);
    })
  );

  listeners.push(
    appWindow.listen<string>("lcu_summoner_name", (e) => {
      setCurrentSummoner(e.payload);
    })
  );

  listeners.push(
    listen("lcu-message", (e) => {
      let parsed = JSON.parse(e.payload as string);

      console.log(parsed);

      if (parsed[2].uri === "/lol-champ-select/v1/pickable-champion-ids" && parsed[2].eventType !== "Delete") {
        setIsOnChampSelect(true);
        return;
      }

      if (parsed[2].uri === "/lol-lobby/v2/lobby") {
        if (parsed[2].eventType === "Delete") {
          console.log("Lobby deleted");
          setSummoners([]);
          return;
        }

        setGamemode(parsed[2].data.gameConfig.queueId);
        return;
      }

      if (parsed[2].uri !== "/lol-lobby/v2/lobby/members") return;

      setSummoners(
        parsed[2].data.map(
          ({ summonerName, summonerIconId }: ISummonerData) => ({
            summonerName,
            summonerIconId,
          })
        )
      );
    })
  );

  return (
    <div class="w-full h-full grid grid-rows-[auto_auto_1fr] bg-dark-9 text-truegray-1">
      <TitleBar />
      <div class="w-full flex content-center flex-col gap-4 p-6">
        <div>
          <div class="w-full h-80 overflow-hidden relative mb-5">
            <div class="rounded-lg w-full h-full bg-gradient-to-t from-black/50 to-transparent absolute"></div>
            <img
              width="700"
              src="/splash.jpg"
              class="w-full h-full object-cover rounded-lg object-center"
            />
          </div>
          <div class="flex items-center gap-2 font-semibold w-full mb-3 text-3xl">
            <div class="i-mdi-users" />
            <span>INVOCADORES:</span>
            <div class="h-full flex px-6 text-sm gap-2 items-center ml-auto my-auto font-semibold text-gray-3">
              <span
                class={`h-2 w-2 ${
                  online() ? "bg-green" : "bg-truegray-4"
                } rounded-full`}
              ></span>
              <span>
                {online()
                  ? `Online${
                      currentSummoner() ? " (" + currentSummoner() + ")" : ""
                    }`
                  : "Offline"}
              </span>
            </div>
          </div>
          <div class="w-full p-2 flex flex-wrap gap-2">
            <Show
              when={summoners().length > 0}
              fallback={
                <div
                  onclick={() => invoke("create_lobby")}
                  class={`bg-white/1 justify-center flex h-50 min-w-38 gap-4
                    flex-col items-center rounded-lg px-3 border-2 select-none
                    border-solid transition-250 text-truegray-3
                    ${
                      currentSummoner()
                        ? "cursor-pointer border-indigo-8 hover:(shadow-[0_0_2em_0.2em_rgba(79,70,229,0.3)])"
                        : "opacity-35 border-truegray-6 cursor-default"
                    }`}
                  aria-disabled={!online()}
                >
                  <div class="rounded-full h-20 w-20">
                    <div class="i-mdi-package-variant-plus w-full h-full"></div>
                  </div>
                  <div class="text-center text-sm font-semibold">
                    Criar Lobby
                  </div>
                </div>
              }
            >
              <For each={summoners()}>
                {(summoner) => (
                  <div
                    class="bg-white/1 justify-evenly flex h-50 min-w-38
                flex-col items-center rounded-lg px-3 border-2 border-indigo-8 border-solid"
                  >
                    <img
                      src={`https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/profile-icons/${summoner.summonerIconId}.jpg`}
                      class="rounded-full h-20 w-20 border-2 border-solid border-amber-6"
                    />
                    <div class="text-center text-sm">
                      {summoner.summonerName}
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>
        {/* <For each={jsons()}>
      {(json) => (
        <div
          class="bg-dark-8 w-full h-full whitespace-pre p-2 rounded"
          innerHTML={json}
        />
      )}
    </For> */}
      </div>
    </div>
  );
}

export default App;
