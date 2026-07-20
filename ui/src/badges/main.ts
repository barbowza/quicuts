import "../app.css";
import { mount } from "svelte";
import Badges from "./Badges.svelte";

mount(Badges, { target: document.getElementById("app")! });
