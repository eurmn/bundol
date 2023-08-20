/* @refresh reload */
import { render } from "solid-js/web";

import "./styles.css";
import App from "./App";
import "virtual:uno.css";
import "@fontsource-variable/rubik";

render(() => <App />, document.getElementById("root") as HTMLElement);
