import { LineChart } from "echarts/charts";
import { GridComponent } from "echarts/components";
import { init, use, type ECharts } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";

use([LineChart, GridComponent, CanvasRenderer]);

export function createWaitChart(el: HTMLDivElement): ECharts {
  return init(el);
}

export type { ECharts };
