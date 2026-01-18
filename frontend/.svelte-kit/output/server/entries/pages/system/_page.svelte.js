import { y as head } from "../../../chunks/index.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    head("hj1sc2", $$renderer2, ($$renderer3) => {
      $$renderer3.title(($$renderer4) => {
        $$renderer4.push(`<title>System - RouterUI</title>`);
      });
    });
    $$renderer2.push(`<div class="space-y-6"><div><h2 class="text-2xl font-bold">System</h2> <p class="text-sm text-gray-500">System logs, updates, and backup management.</p></div> `);
    {
      $$renderer2.push("<!--[-->");
      $$renderer2.push(`<div class="text-gray-400">Loading...</div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
