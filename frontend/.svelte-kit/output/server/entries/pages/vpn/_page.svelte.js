import { y as head, z as attr_class, F as stringify } from "../../../chunks/index.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    head("awi8zj", $$renderer2, ($$renderer3) => {
      $$renderer3.title(($$renderer4) => {
        $$renderer4.push(`<title>VPN - RouterUI</title>`);
      });
    });
    $$renderer2.push(`<div class="space-y-6"><div><h2 class="text-2xl font-bold">VPN</h2> <p class="text-sm text-gray-500">Remote access and download VPN connections.</p></div> <div class="flex gap-2 border-b border-gray-700 pb-2"><button${attr_class(`px-4 py-2 rounded-t ${stringify(
      "bg-gray-700 text-white"
    )}`)}>Remote Access</button> <button${attr_class(`px-4 py-2 rounded-t ${stringify("text-gray-400 hover:text-white")}`)}>Download VPN</button></div> `);
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
