# Tsky - Bluesky in Terminal

## IMPORTANT: This is an application written for myself.

I will prioritize features I use daily, and most likely ignore features I
don't use.

## Features implemented

- Viewing Following Feed
    - Like
    - repost
    - open post in bsky.app
    - view image with feh
    - open links in browser
    - watch video using VLC
- Viewing post threads
- Labels
- Auto updating feed every second
- Posting (no rich text and embeds yet)
    - subset of vim keybindings

## TODO List

- Reply
- Quote
- Post embed
- Different sources of feed
- Multicolumn
- Config file (default layout, post languages, how to open media, feed update frequency)
- Notification
- View profile

## Login

For first time login, set `handle` and `password` for enviornment variable. You
may want to put the variables in `$PWD/.env`. I don't have my own PDS so I
don't know if logging in with PDS other than official one will work or not.

`session.json` will be created in directory `$HOME/.local/share/tsky/`. I don't
care `XDG_CONFIG_HOME`. Necessary directories will be created automatically.

From now on, log in will be using `session.json` so _supposedly_ no environment
variables will need to be set.

## Caveats

As the feed gets longer and longer, updating feed will take more computational
power as it uses `O(n)` algorithm to merge two new posts into old posts. It is
not recommeneded to open the client for too long.

Obviously the code is not optimized anyways.

## Controls

Feed / Thread view

| key | function |
| - | - |
| `q` | quit |
| `j` | next post |
| `k` | previous post |
| `space` | like post |
| `o` | repost post |
| `p` | open post in browser |
| `m` | open media (VLC is required to watch video) |
| `enter` | open post thread view |
| `enter (in thread view)` | open embeded post (if any) |
| `backspace` | go back to previous view |
| `n (in feed view)` | new post |
| `f (in thread view)` | show list of link facets |

Composer view

| key | function |
| - | - |
| `tab` | switch between focusing text field and langs field |
| `enter` | post |
| `backspace` | go back to previous view |
