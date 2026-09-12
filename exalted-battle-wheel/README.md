# Exalted Battle Wheel

## Run in debug mode

```
trunk serve
```

## Build for release

```
trunk build --release
```

## Deploy

One-time infrastructure setup (state bucket `jeffs-tfstate` must already exist):

```
export AWS_PROFILE=personal
terraform -chdir=terraform init
terraform -chdir=terraform apply
```

Every subsequent deploy:

```
export AWS_PROFILE=personal
./deploy.sh
```

`deploy.sh` builds a release bundle, syncs it to S3, and invalidates the CloudFront cache. See `terraform/` for the infrastructure and `CLAUDE.md` for how the hosting is wired together.
