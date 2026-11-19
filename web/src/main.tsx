import { render } from "preact";
import { adoptTokenFromUrl } from "./api";
import { App } from "./app";
import "./styles.css";

adoptTokenFromUrl();
render(<App />, document.getElementById("app")!);
