# QuarkTRM - Quark Drive Auto-Save Tool

A Rust implementation of automatic file transfers for Quark Drive (夸克网盘). This tool allows you to automatically transfer files from shared links to your own Quark Drive account, with support for file filtering, renaming, and organization.

Inspired by [Cp0204/quark-auto-save](https://github.com/Cp0204/quark-auto-save), but implemented in Rust without requiring a WebUI.

## Features

- **Authentication**: Use your Quark Drive cookie for authentication
- **Task-based Operation**: Configure multiple tasks with different sources and destinations
- **Regular Expression Support**: Filter files using regex patterns
- **File Renaming**: Rename files using regex replacement patterns
- **Scheduled Execution**: Run tasks on specific days of the week
- **Notification Support**: Send notifications via Pushover, Telegram, Discord, or custom webhooks
- **Daily Sign-in**: Automatically sign in to earn storage rewards
- **Command-line Interface**: All functionality available through CLI commands

## Installation

### Prerequisites

- [Rust and Cargo](https://www.rust-lang.org/tools/install) (1.70.0 or newer recommended)

### Building from Source

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/quarktrm.git
   cd quarktrm
   ```

2. Build the project:
   ```
   cargo build --release
   ```

3. The compiled binary will be available at `target/release/quarktrm`

## Usage

### Initialize Configuration

Create a new configuration file:

```bash
quarktrm --config config.json init
```

Edit the generated configuration file with your Quark Drive authentication and tasks.

### Run Tasks

Run all scheduled tasks:

```bash
quarktrm --config config.json run
```

Run a specific task:

```bash
quarktrm --config config.json run --task-id my-task-id
```

### Daily Sign-in

Perform daily sign-in to earn storage rewards:

```bash
quarktrm --config config.json sign-in
```

### Check Share

Check a share link's validity and contents:

```bash
quarktrm --config config.json check-share --url "https://pan.quark.cn/s/example" --code "extraction-code"
```

### List Tasks

List all configured tasks:

```bash
quarktrm --config config.json list-tasks
```

### Account Information

Show your account information:

```bash
quarktrm --config config.json account-info
```

## Configuration

The configuration is stored in a JSON file. Here's an example configuration:

```json
{
  "auth": {
    "cookie": "your_quark_cookie_string",
    "access_token": null,
    "user_agent": null
  },
  "tasks": [
    {
      "id": "tv-shows",
      "name": "TV Shows",
      "target_directory": "/path/to/tv-shows",
      "create_directory": true,
      "end_date": null,
      "subtasks": [
        {
          "share_url": "https://pan.quark.cn/s/example1",
          "extraction_code": null,
          "subdirectory": null,
          "file_pattern": "$TV",
          "rename_pattern": "Show.S01E$1.mp4",
          "ignore_extension": false,
          "run_on_days": ["Monday", "Wednesday", "Friday"]
        }
      ],
      "enable_notifications": true,
      "refresh_media_library": false
    }
  ],
  "notification": {
    "enabled": true,
    "pushover": {
      "api_token": "your_pushover_api_token",
      "user_key": "your_pushover_user_key"
    },
    "telegram": null,
    "discord": null,
    "webhook": null
  },
  "media_library": null,
  "options": {
    "data_dir": null,
    "enable_daily_signin": true,
    "user_agent": null,
    "max_concurrent_tasks": 2,
    "variables": {}
  }
}
```

### Authentication

You need to provide your Quark Drive authentication cookie in the configuration file. You can get this by:

1. Log in to [Quark Drive](https://pan.quark.cn/) in your browser
2. Open browser developer tools (F12)
3. Go to the "Network" tab
4. Refresh the page
5. Find any request to `quark.cn`
6. Copy the entire `Cookie` header value

### Task Configuration

Each task has the following properties:

- `id`: Unique identifier for the task
- `name`: Human-readable name
- `target_directory`: Where to save transferred files
- `create_directory`: Whether to create the target directory if it doesn't exist
- `end_date`: Optional date after which the task will no longer run
- `subtasks`: List of share URLs and processing instructions
- `enable_notifications`: Whether to send notifications for this task
- `refresh_media_library`: Whether to refresh media libraries after this task

### Subtask Configuration

Each subtask has the following properties:

- `share_url`: Quark Drive share URL
- `extraction_code`: Extraction code for protected shares (if needed)
- `subdirectory`: Optional subdirectory within the share to process
- `file_pattern`: Regular expression pattern to match files
- `rename_pattern`: Replacement pattern for renaming files
- `ignore_extension`: Whether to ignore file extensions during pattern matching
- `run_on_days`: Days of the week this task should run

## Regular Expression Examples

### File Patterns

| Pattern | Description | Example Match |
|---------|-------------|---------------|
| `.*` | Match all files | Any file |
| `\.mp4$` | Match files ending with .mp4 | video.mp4 |
| `^Episode\d+` | Match files starting with "Episode" followed by numbers | Episode01.mp4 |
| `$TV` | Magic pattern to match common video formats | any .mp4, .mkv, .avi, etc. |
| `$MOVIE` | Magic pattern for movie files | any .mp4, .mkv, .avi, etc. |
| `$AUDIO` | Magic pattern for audio files | any .mp3, .flac, .aac, etc. |

### Rename Patterns

| Original Name | Pattern | Result | Explanation |
|---------------|---------|--------|-------------|
| episode-01.mp4 | `Episode $1.mp4` | Episode 01.mp4 | Extract the number |
| 01.mp4 | `S01E$1.mp4` | S01E01.mp4 | Add season prefix |
| show-s01e01.mp4 | `{TASKNAME}.$1` | TaskName.s01e01.mp4 | Use task name as prefix |

## Magic Patterns

QuarkTRM supports several "magic patterns" that expand to commonly used regular expressions:

- `$TV` - Common TV/video formats
- `$MOVIE` - Common movie formats
- `$VIDEO` - All video formats
- `$AUDIO` - Audio formats
- `$IMAGE` - Image formats
- `$DOCUMENT` - Document formats
- `$ARCHIVE` - Archive formats
- `$SUBTITLE` - Subtitle formats
- `$ALL` - Match everything

## Magic Variables

You can use these variables in rename patterns:

- `{TASKNAME}` - Name of the current task

## License

This project is licensed under the AGPL-3.0 License - see the LICENSE file for details.

## Acknowledgements

- [Cp0204/quark-auto-save](https://github.com/Cp0204/quark-auto-save) - The original Python implementation that inspired this project