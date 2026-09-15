#!/usr/bin/env bash
# Builds nothing -- pushes an already-built local image into the cluster's containerd, with no
# registry involved. Usage: ./push-image.sh <image:tag>
#
# Streams `docker save` straight into `k0s ctr images import` over the SSM tunnel (piped through
# gzip -1): nothing is staged on the instance's disk, so there's no temp file and no cleanup step to
# fail on a box that's already tight on space. gzip -1 is the right trade over the ~1MB/s SSM
# websocket -- it's latency-bound, not CPU-bound.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
source ./_ssm.sh

image="${1:?usage: ./push-image.sh <image:tag>}"

if [[ "$image" != *:* ]] || [[ "$image" == *:latest ]]; then
  echo "refusing to push '$image': tag it with something other than :latest or no tag," >&2
  echo "or kubelet will default imagePullPolicy to Always and try Docker Hub instead of this image" >&2
  exit 1
fi

# The instance is arm64; a laptop building amd64 images here is the most common first mistake.
arch="$(docker image inspect --format '{{.Architecture}}' "$image")"
if [ "$arch" != "arm64" ]; then
  echo "image '$image' is $arch, need arm64 -- build with:" >&2
  echo "  docker buildx build --platform linux/arm64 -t $image --load ." >&2
  exit 1
fi

wait_for_ssm
echo "pushing $image..." >&2
docker save "$image" | gzip -1 |
  ssm_ssh "ubuntu@$instance_id" 'sudo /usr/local/bin/k0s ctr images import --platform linux/arm64 -'

echo "imported. resolved ref(s) in containerd:" >&2
ref="${image#*/}"
ssm_ssh "ubuntu@$instance_id" 'sudo /usr/local/bin/k0s ctr images ls -q' | grep "$ref" || true
echo "use the exact ref above in your manifest's image: field, with imagePullPolicy: IfNotPresent" >&2
