import assert from "node:assert/strict";
import { test } from "node:test";
import { decode, encode } from "cbor2";
import { v1MessageSchema } from "./message";

void test("decodes only valid vehicle paths before forwarding the frame", () => {
  const vehicle = ["v", "1", "trip", 45.8, 16];
  const row = [...vehicle, ...Array<null>(9).fill(null)];
  const maxSize = Array<number>(512).fill(0);
  const frame = encode({
    v: 1,
    d: {
      vehicles: [
        [...row, [16, 45.8, 16.1, 45.9]],
        vehicle,
        [...row, maxSize],
        [...row, Array<number>(514).fill(0)],
        [...row, [16, 45.8, 16.1]],
        [...row, [16, Infinity]],
        [...row, [16, 91]],
      ],
    },
  });
  const message = v1MessageSchema.parse(decode(frame));

  assert("vehicles" in message.d);
  const [valid, old, largest, ...invalid] = message.d.vehicles;
  assert.deepEqual(valid?.path, [
    [16, 45.8],
    [16.1, 45.9],
  ]);
  assert.deepEqual(old?.path, []);
  assert.equal(largest?.path.length, 256);
  assert.deepEqual(
    invalid.map((vehicle) => vehicle.path),
    [[], [], [], []],
  );
});
