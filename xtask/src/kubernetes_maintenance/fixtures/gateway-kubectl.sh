#!/bin/sh
set -eu
fixture_directory="$HIRUNE_GATEWAY_FIXTURE_DIRECTORY"
while [ "$1" != get ] && [ "$1" != exec ] && [ "$1" != patch ]; do shift; done
case "$1 $2" in
  'get pods')
    printf '{"items":[{"metadata":{"name":"gateway-first"},"status":{"phase":"Running"}},{"metadata":{"name":"gateway-delayed"},"status":{"phase":"Running"}}]}' ;;
  'patch configmap')
    case "$*" in *off*) : ;; *) exit 1 ;; esac
    printf off > "$fixture_directory/maintenance"
    printf 'maintenance:off\n' >> "$fixture_directory/events" ;;
  'exec gateway-first'|'exec gateway-delayed')
    case "$*" in
      */health/ready*)
        if [ "$2" = gateway-delayed ] && [ ! -f "$fixture_directory/delayed" ]; then
          printf attempted > "$fixture_directory/delayed"
          printf 'delayed:%s\n' "$(/bin/cat "$fixture_directory/maintenance")" >> "$fixture_directory/events"
          printf 'HTTP/1.0 502 Bad Gateway\r\nContent-Type: text/html\r\n\r\n<html>private-readiness-response</html>'
        else
          printf 'ready:%s\n' "$2" >> "$fixture_directory/events"
          printf 'HTTP/1.0 204 No Content\r\n\r\n'
        fi ;;
      */health/gateway*)
        enabled=false
        if [ "$(/bin/cat "$fixture_directory/maintenance")" = on ]; then enabled=true; fi
        printf 'HTTP/1.0 200 OK\r\n\r\n{"maintenance":%s}' "$enabled" ;;
      *) exit 1 ;;
    esac ;;
  *) exit 1 ;;
esac
