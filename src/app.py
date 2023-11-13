import os
import json
import sys
import pystray
import scheduler
import logging
import setup
import webbrowser
import subprocess
from hydroserverpy import HydroServer
from PIL import Image
from appdirs import user_data_dir
from logging.handlers import RotatingFileHandler


class HydroLoaderApp:

    def __init__(self, setup_window):
        base_path = getattr(sys, '_MEIPASS', 'assets')

        image = Image.open(os.path.join(base_path, 'app_icon.png'))

        menu = (
            pystray.MenuItem('Streaming Data Loader is running', lambda: None, enabled=False),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem('Data Sources Dashboard', self.open_data_sources_dashboard),
            # pystray.MenuItem('Update Settings', self.update_settings),
            pystray.MenuItem('View Log Output', self.open_logs),
            pystray.MenuItem('Quit Application', self.close_app)
        )

        self.tray = pystray.Icon('hydroloader', image, 'Streaming Data Loader', menu)
        self.setup = setup_window
        self.app_dir = user_data_dir('Streaming Data Loader', 'CIROH')

        if not os.path.exists(self.app_dir):
            os.makedirs(self.app_dir)

        self.hydroloader_instance = None
        self.hydroserver_username = None
        self.hydroserver_password = None
        self.hydroserver_url = None

    def open_data_sources_dashboard(self):
        webbrowser.open(f'{self.hydroserver_url}/data-sources')

    def get_settings(self):
        settings_path = os.path.join(self.app_dir, 'settings.json')
        if os.path.exists(settings_path):
            with open(settings_path, 'r') as settings_file:
                settings = json.loads(settings_file.read() or 'null') or {}
                self.hydroserver_url = settings.get('url')
                self.hydroloader_instance = settings.get('instance')
                self.hydroserver_username = settings.get('username')
                self.hydroserver_password = settings.get('password')

    def update_settings(self):
        pass

    def open_logs(self):
        subprocess.call(['open', os.path.join(self.app_dir, 'sdl.log')])

    def launch_background(self):
        self.get_settings()

        if all([
            self.hydroloader_instance, self.hydroserver_username, self.hydroserver_password
        ]):
            self.setup.withdraw()
            service = HydroServer(
                host=self.hydroserver_url,
                auth=(
                    self.hydroserver_username,
                    self.hydroserver_password
                )
            )
            scheduler.HydroLoaderScheduler(
                service=service,
                instance=self.hydroloader_instance,
            )
            self.tray.run_detached()

    def launch_app(self):
        self.setup.after(0, self.launch_background)
        self.setup.mainloop()

    def close_app(self):
        self.tray.stop()
        # self.setup.destroy()
        # sys.exit(0)
        os._exit(0)  # There's a Windows ctk bug causing the app to not shutdown with normal methods.


if __name__ == '__main__':

    hydroloader_logger = logging.getLogger('hydroloader')
    scheduler_logger = logging.getLogger('scheduler')

    stream_handler = logging.StreamHandler()
    hydroloader_logger.addHandler(stream_handler)
    scheduler_logger.addHandler(stream_handler)

    user_dir = user_data_dir('Streaming Data Loader', 'CIROH')

    if not os.path.exists(user_dir):
        os.makedirs(user_dir)

    log_path = os.path.join(user_dir, 'sdl.log')

    log_handler = RotatingFileHandler(
        filename=log_path,
        mode='a',
        maxBytes=20 * 1024 * 1024,
        backupCount=3
    )
    hydroloader_logger.addHandler(log_handler)
    scheduler_logger.addHandler(log_handler)

    logging.basicConfig(
        format='%(asctime)s %(levelname)-8s %(message)s',
        level=logging.INFO,
        datefmt='%Y-%m-%d %H:%M:%S',
        force=True,
        handlers=[
            log_handler, stream_handler
        ]
    )

    default_hydroserver_url = 'https://www.hydroserver.org'

    hydroloader_setup = setup.AppSetup(default_hydroserver_url=default_hydroserver_url)
    hydroloader = HydroLoaderApp(setup_window=hydroloader_setup)
    hydroloader_setup.callback = hydroloader.launch_background
    hydroloader.launch_app()
