# kubernetes-host

A single EC2 instance running k0s, reachable at any `*.jeffrey.lol` subdomain. See `CLAUDE.md` for how
it's wired together.

## Prerequisites

```
yay -S kubectl docker docker-buildx aws-session-manager-plugin
```

```
mkdir ssh
ssh-keygen -t ed25519 -f ssh/kubernetes-host -N ""
```

Running cost is about $18/mo: ~$12 for the instance, ~$3.60 for the Elastic IP (AWS bills all in-use
public IPv4 addresses), ~$2.40 for the 30GB root volume.

## Common environment variables

```
export AWS_PROFILE=personal
export AWS_REGION=us-east-1
```

## Stand up

```
terraform -chdir=terraform init
terraform -chdir=terraform apply
```

A brand-new AWS account can fail the very first apply on instance-profile propagation delay -- just
re-run it.

Then, in one terminal:

```
./kubeconfig.sh
./tunnel.sh
```

And in another:

```
export KUBECONFIG=$PWD/kubeconfig
kubectl get nodes
```

`kubectl get nodes` should show one `Ready` node within a few minutes of the instance booting.
`kubectl -n cert-manager get pods` and `kubectl -n ingress-nginx get pods` should all reach `Running`
around the same time, and `kubectl get clusterissuer` should show both issuers `Ready` shortly after
(a few minutes of `False` while cert-manager's webhook comes up is normal).

## Deploy a test service end-to-end

`example/` is a minimal nginx site. The instance is arm64, so the image must be built for that
platform explicitly:

```
docker buildx build --platform linux/arm64 -t example:v1 --load example/
./push-image.sh example:v1
kubectl apply -f example/manifest.yaml
```

Wait for the certificate (staging, by default -- see the annotation in `example/manifest.yaml`):

```
kubectl get certificate -w
```

Once it shows `READY True`:

```
curl -k https://example.jeffrey.lol
```

(`-k` because the staging issuer's cert isn't trusted by your local CA store -- that's expected. Flip
the Ingress's `cert-manager.io/cluster-issuer` annotation to `letsencrypt-prod`, `kubectl apply` again,
wait for a new certificate, and `curl` without `-k` once you're done testing and want a browser-trusted
cert.)

Tear the example down:

```
kubectl delete -f example/manifest.yaml
```

## Tear down

```
terraform -chdir=terraform destroy
```
