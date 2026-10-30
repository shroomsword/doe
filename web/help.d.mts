export const SECTIONS: string[];
export function buildHelp(readme: string, sections?: string[]): { html: string; headings: { level: number; id: string; text: string }[] };
