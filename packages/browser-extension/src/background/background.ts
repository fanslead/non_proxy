import { getBrowserAPI } from "../platform/browser-api.js";
import { BackgroundApp } from "./background-app.js";
import { LearningController } from "./learning-controller.js";
import { NativePortClient } from "./native-port-client.js";

const browser = getBrowserAPI();
let app: BackgroundApp;
const learning = new LearningController(
  new NativePortClient(browser),
  undefined,
  (count) => {
    if (count === 0) {
      void app.releaseLearningPermission();
    }
  },
);
app = new BackgroundApp(browser, learning);
app.install();
