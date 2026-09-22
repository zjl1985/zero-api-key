import { render } from "preact";
import App from "./App";
import { applyTheme, loadTheme } from "./theme";

applyTheme(loadTheme());
render(<App />, document.getElementById("root")!);
