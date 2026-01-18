
// this file is generated — do not edit it


declare module "svelte/elements" {
	export interface HTMLAttributes<T> {
		'data-sveltekit-keepfocus'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-noscroll'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-preload-code'?:
			| true
			| ''
			| 'eager'
			| 'viewport'
			| 'hover'
			| 'tap'
			| 'off'
			| undefined
			| null;
		'data-sveltekit-preload-data'?: true | '' | 'hover' | 'tap' | 'off' | undefined | null;
		'data-sveltekit-reload'?: true | '' | 'off' | undefined | null;
		'data-sveltekit-replacestate'?: true | '' | 'off' | undefined | null;
	}
}

export {};


declare module "$app/types" {
	export interface AppTypes {
		RouteId(): "/" | "/adguard" | "/antivirus" | "/docker" | "/firewall" | "/media" | "/network" | "/protection" | "/security" | "/services" | "/system" | "/users" | "/vpn";
		RouteParams(): {
			
		};
		LayoutParams(): {
			"/": Record<string, never>;
			"/adguard": Record<string, never>;
			"/antivirus": Record<string, never>;
			"/docker": Record<string, never>;
			"/firewall": Record<string, never>;
			"/media": Record<string, never>;
			"/network": Record<string, never>;
			"/protection": Record<string, never>;
			"/security": Record<string, never>;
			"/services": Record<string, never>;
			"/system": Record<string, never>;
			"/users": Record<string, never>;
			"/vpn": Record<string, never>
		};
		Pathname(): "/" | "/adguard" | "/adguard/" | "/antivirus" | "/antivirus/" | "/docker" | "/docker/" | "/firewall" | "/firewall/" | "/media" | "/media/" | "/network" | "/network/" | "/protection" | "/protection/" | "/security" | "/security/" | "/services" | "/services/" | "/system" | "/system/" | "/users" | "/users/" | "/vpn" | "/vpn/";
		ResolvedPathname(): `${"" | `/${string}`}${ReturnType<AppTypes['Pathname']>}`;
		Asset(): string & {};
	}
}