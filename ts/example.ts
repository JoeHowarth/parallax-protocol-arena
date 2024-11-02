// example.ts

import { recv, send } from "./api.ts";

const msgs: string[] = [];

runjs.sleep(1).then(async () => {
	while (true) {
		const joined = msgs.join(", ");
		await send({
			"Msg": joined,
		});
		await runjs.sleep(1000);
	}
});

async function main() {
	while (true) {
		const result = await recv();
		console.log(result);
		msgs.push(result.Msg);
	}
}

main().catch((e) => {
	console.error(e);
});
