/* @refresh reload */
import { render } from "solid-js/web";

import "./styles.css";
import App from "./routes/App";
import "virtual:uno.css";
import "@fontsource-variable/rubik";
import { Router, Route, Routes } from "@solidjs/router";
import { Updater } from "./routes/Updater";

render(
  () => (
    <Router>
      <Routes>
        <Route path="/" component={App} />
        <Route path="/updater" component={Updater} />
      </Routes>
    </Router>
  ),
  document.getElementById("root") as HTMLElement
);
