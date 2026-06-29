import { z } from "zod";
import { versionedSchema } from "../versioned";

const noticeSchema = z.object({
  id: z.string(),
  text: z.string(),
  severity: z.enum(["info", "warning", "error"]),
});

export const v1MessageSchema = versionedSchema(
  1,
  z
    .object({
      vehicles: z.array(
        z.tuple([z.string(), z.string(), z.string(), z.number(), z.number()]).rest(z.unknown()),
      ),
    })
    .or(
      z.object({
        activeStops: z.array(z.string()),
      }),
    )
    .or(
      z.object({
        notices: z.array(noticeSchema),
      }),
    )
    .or(
      z.object({
        userNotices: z.array(noticeSchema),
      }),
    )
    .or(
      z.object({
        toast: z.object({
          message: z.string(),
          type: z.enum(["info", "success", "warning", "error"]),
          duration: z.number().optional(),
        }),
      }),
    )
    .or(
      z.object({
        stopIds: z.array(z.string()),
        route: z.array(z.tuple([z.number(), z.number()])),
      }),
    )
    .or(
      z.object({
        simpleStops: z.array(z.tuple([z.string(), z.string(), z.number(), z.number()])),
      }),
    )
    .or(
      z.object({
        stopTrips: z.array(z.string()),
      }),
    )
    .or(
      z.object({
        gbfsStations: z.array(
          z.tuple([z.string(), z.string().nullable(), z.number(), z.number()]).rest(z.unknown()),
        ),
      }),
    ),
);

export type V1Message = z.infer<typeof v1MessageSchema>;
export type GlobalNotice = z.infer<typeof noticeSchema>;
