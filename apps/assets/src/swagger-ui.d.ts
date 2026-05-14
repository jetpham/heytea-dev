declare module "swagger-ui-dist/swagger-ui-bundle.js" {
  type SwaggerOptions = {
    dom_id: string;
    url: string;
  };

  export default function SwaggerUI(options: SwaggerOptions): unknown;
}
