import SwaggerUI from "swagger-ui-dist/swagger-ui-bundle.js";
import "swagger-ui-dist/swagger-ui.css";

SwaggerUI({
  dom_id: "#swagger-ui",
  url: "https://api.heytea.dev/openapi.json",
});
