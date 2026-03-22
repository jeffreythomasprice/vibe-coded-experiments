import { XenForoAdapter } from "./xenforo";

export class SufficientVelocityAdapter extends XenForoAdapter {
    constructor() {
        super({
            siteName: "sufficientvelocity",
            baseUrl: "https://forums.sufficientvelocity.com",
            tagCategories: {
                fandom: "tagItem--cat_4",
                genre: "tagItem--cat_7",
                character: "tagItem--cat_10",
            },
        });
    }
}
