#!/usr/bin/env bash
# Spot test VMs on Google Cloud for cross-platform runs.
# A VM stops itself after MAX_RUN (and when Google reclaims the Spot capacity); its disk with the
# build and the models stays, so `start` continues where it stopped. A stopped VM costs only its
# disk; `delete` removes the VM and its disk.
#
#   test/cloud/vm.sh create <name> <machine> <gpu-type|none> [image-family]
#   test/cloud/vm.sh start <name>        (after a stop or a Spot reclaim)
#   test/cloud/vm.sh stop <name>
#   test/cloud/vm.sh ssh <name> '<command>'
#   test/cloud/vm.sh put <name> <local-file> <remote-path>
#   test/cloud/vm.sh get <name> <remote-path> <local-dir>
#   test/cloud/vm.sh delete <name>
#   test/cloud/vm.sh list
set -euo pipefail

ZONES="${ZONES:-us-central1-a us-central1-b us-central1-c us-central1-f}"
MAX_RUN="${MAX_RUN:-3h}"
DISK_GB="${DISK_GB:-100}"
IMAGE_PROJECT="${IMAGE_PROJECT:-deeplearning-platform-release}"
ZONE_FILE="${TMPDIR:-/tmp}/cluaiz-vm-zones"

zone_of() { grep "^$1 " "$ZONE_FILE" 2>/dev/null | tail -1 | cut -d' ' -f2; }

case "${1:-}" in
  create)
    name="$2" machine="$3" gpu="$4" family="${5:-common-cu129-ubuntu-2404-nvidia-580}"
    accel=()
    [ "$gpu" != none ] && accel=(--accelerator="count=1,type=$gpu" --metadata=install-nvidia-driver=True)
    for zone in $ZONES; do
      echo "== trying $zone"
      if gcloud compute instances create "$name" --zone="$zone" --machine-type="$machine" "${accel[@]}" \
          --image-family="$family" --image-project="$IMAGE_PROJECT" \
          --boot-disk-size="${DISK_GB}GB" --boot-disk-type=pd-balanced \
          --provisioning-model=SPOT --instance-termination-action=STOP --max-run-duration="$MAX_RUN" \
          --maintenance-policy=TERMINATE --labels=purpose=cluaiz-test; then
        echo "$name $zone" >> "$ZONE_FILE"
        exit 0
      fi
    done
    echo "no zone had capacity" >&2; exit 1 ;;
  start)  gcloud compute instances start "$2" --zone="$(zone_of "$2")" ;;
  stop)   gcloud compute instances stop "$2" --zone="$(zone_of "$2")" ;;
  ssh)    gcloud compute ssh "$2" --zone="$(zone_of "$2")" --command="$3" ;;
  put)    gcloud compute scp "$3" "$2:$4" --zone="$(zone_of "$2")" ;;
  get)    gcloud compute scp --recurse "$2:$3" "$4" --zone="$(zone_of "$2")" ;;
  delete) gcloud compute instances delete "$2" --zone="$(zone_of "$2")" --quiet ;;
  list)   gcloud compute instances list ;;
  *)      sed -n 2,15p "$0"; exit 1 ;;
esac
