import { z } from "zod";

export const versionedSchema = <TData, TVersion extends number>(
  version: TVersion,
  dataSchema: z.ZodType<TData, z.ZodTypeDef, unknown>,
) =>
  z.object({
    v: z.literal(version),
    ts: z.number().optional(),
    d: dataSchema,
  });

export type Versioned<T, Version extends number = number> = {
  v: Version;
  ts: number | undefined;
  d: T;
};
