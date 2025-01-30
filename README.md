# Tsky - Bluesky in Terminal

## IMPORTANT: This is an application written for myself.

I will prioritize features I use daily, and most likely ignore features I
don't use.

![Works on my machine](https://blog.codinghorror.com/content/images/uploads/2007/03/6a0120a85dcdae970b0128776ff992970c-pi.png)

## Dependencies

- feh
- vlc
- zenity
- wl-paste (wayland / wlroots)

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
- Posting
    - subset of vim keybindings
    - embed
        - images
        - link card
        - quote
    - paste image from clipboard
- View profile
- Notification

## TODO List

- Different sources of feed
- Multicolumn
- Config file (default layout, post languages, how to open media, feed update frequency)

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

### Feed

| key | function |
| - | - |
| `q` | quit |
| `j` | next post |
| `k` | previous post |
| `enter` | open post thread view |
| `enter (in thread view)` | open embeded post (if any) |
| `n (in feed view)` | new post |
| `b` | open notifications |
| `backspace` | go back to previous view |

### Post objects

| key | function |
| - | - |
| `space` | like post |
| `o` | repost post |
| `p` | open post in browser |
| `a` | open author profile page |
| `A` | open reposting author profile page |
| `m` | open media |
| `f` | show list of link facets |
| `F` | show likes |

### Actor objects

| key | function |
| - | - |
| `a` | open profile page |
| `p (in profile page)` | open profile page in browser |
| `m (in profile page)` | open avatar and banner |
| `enter (in profile page)` | follow actor |

### Notification objects

| key | function |
| - | - |
| `A` | open notification author profile page |

### Composer

| key | function |
| - | - |
| `tab` | switch focus between fields |
| `enter` (text / lang) | post |
| `enter` (embed) | open file picker |
| `backspace` | go back to previous view |
| `j / k` | navigate embed items |
| `dd` | remove embed item |
