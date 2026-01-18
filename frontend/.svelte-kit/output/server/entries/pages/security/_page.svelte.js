import { y as head, x as attr } from "../../../chunks/index.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let autoRefresh = true;
    head("4rm2pb", $$renderer2, ($$renderer3) => {
      $$renderer3.title(($$renderer4) => {
        $$renderer4.push(`<title>Security - RouterUI</title>`);
      });
    });
    $$renderer2.push(`<div class="space-y-6"><div class="flex items-center justify-between"><div><h2 class="text-2xl font-bold">Security Monitor</h2> <p class="text-sm text-gray-500">Real-time security events and threat monitoring</p></div> <label class="flex items-center gap-2 text-sm"><input type="checkbox"${attr("checked", autoRefresh, true)} class="rounded"/> <span class="text-gray-400">Auto-refresh (30s)</span></label></div> `);
    {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="text-gray-400">Loading security data...</div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
