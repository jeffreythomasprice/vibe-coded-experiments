CREATE TABLE "provider_mounts" (
	"mount_id" text PRIMARY KEY NOT NULL,
	"scheme" text NOT NULL,
	"config" jsonb NOT NULL,
	"created_at" timestamp DEFAULT now() NOT NULL
);
