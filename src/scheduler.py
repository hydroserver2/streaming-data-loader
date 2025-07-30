import traceback
import logging
from apscheduler.schedulers.background import BackgroundScheduler
from apscheduler.triggers.cron import CronTrigger
from apscheduler.triggers.interval import IntervalTrigger
from pytz import utc
from datetime import datetime
from PySide6.QtCore import QObject


logger = logging.getLogger("scheduler")


class DataLoaderScheduler(QObject):

    def __init__(self, hs_api, data_loader=None):
        super().__init__()

        self.data_loader = data_loader

        self.scheduler = BackgroundScheduler(timezone=utc)
        self.scheduler.add_job(
            lambda: self.check_data_sources(),
            id="sdl-scheduler",
            trigger="interval",
            seconds=60,
            next_run_time=datetime.utcnow()
        )

        logging.getLogger("apscheduler.executors.default").setLevel(logging.WARNING)

        self.hs_api = hs_api
        self.timeout = 60

        self.scheduler.start()
        self.job = None

    def terminate(self):
        self.scheduler.shutdown(wait=True)

    def pause(self):
        if self.scheduler.running:
            self.scheduler.pause()

    def resume(self):
        self.scheduler.resume()

    def check_data_sources(self):
        """
        The check_data_sources function is used to check the status of all data sources associated with a given
        instance. It will iterate through each data source and call the update_data_source function for each one.

        :param self
        :return: The data_sources
        """

        try:
            success, message = self.check_data_loader()
            if success is False:
                logging.error(message)
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)

        try:
            data_sources = self.hs_api.datasources.list(orchestration_system=self.data_loader, fetch_all=True)
            for data_source in data_sources.items:
                self.update_data_source(data_source)
        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)

    def check_data_loader(self):
        """
        The check_data_loader function checks to see if the data loader name provided by the user exists. If it does
        not, it creates a new data loader with that name. If it does exist, then it sets self.data_loader to that
        existing data loader.

        :param self
        :return: A tuple containing a boolean and a string
        """

        try:
            data_loader = self.hs_api.orchestrationsystems.get(uid=self.data_loader.uid)
        except (Exception,) as e:
            return False, str(e)

        self.data_loader = data_loader

        return True, ''

    def update_data_source(self, data_source):
        """
        The update_data_source function is called when a user updates the schedule of an existing data source.
        It checks to see if the data source has a scheduled job, and if it does not, it adds one. If there is already
        a scheduled job for that data source, then update_data_source calls update_schedule to change the schedule.

        :param self
        :param data_source: Identify the data source that is being updated
        :return: bool
        """

        scheduled_jobs = {
            scheduled_job.id: scheduled_job
            for scheduled_job in self.scheduler.get_jobs()
            if scheduled_job.id != 'hydroloader-scheduler'
        }

        if str(data_source.uid) not in scheduled_jobs.keys():
            self.add_schedule(data_source)
        else:
            self.update_schedule(data_source, scheduled_jobs[str(data_source.uid)])

        return True

    def add_schedule(self, data_source):
        """
        The add_schedule function is used to add a schedule for the data source. The function takes in a
        DataSourceGetResponse object as an argument, which contains all the information needed to create and run
        scheduled data loading tasks.

        :param self
        :param data_source: DataSourceGetResponse: Pass the data source object to the function
        :return: None
        """
        schedule_range = {}
        if data_source.start_time:
            schedule_range['start_time'] = data_source.start_time
        if data_source.end_time:
            schedule_range['end_time'] = data_source.end_time

        if data_source.interval and data_source.interval_units:
            self.scheduler.add_job(
                lambda: self.load_data(data_source=data_source),
                IntervalTrigger(
                    start_date=data_source.start_time,
                    end_date=data_source.end_time,
                    **{data_source.interval_units: data_source.interval}
                ),
                id=str(data_source.uid),
                **schedule_range
            )
        elif data_source.crontab:
            self.scheduler.add_job(
                lambda: self.load_data(data_source=data_source),
                CronTrigger.from_crontab(data_source.crontab, timezone='UTC'),
                id=str(data_source.uid),
                **schedule_range
            )

    def update_schedule(self, data_source, scheduled_job):
        """
        The update_schedule function is called when a data source is updated.
        It checks if the crontab or interval has changed, and if so, removes the old job from the scheduler and adds a
        new one. If neither have changed, it does nothing.

        :param self
        :param data_source: DataSourceGetResponse: Get the data source information
        :param scheduled_job: Get the job id and trigger
        :return: None
        """

        if (
            (isinstance(scheduled_job.trigger, CronTrigger) and not data_source.crontab) or
            (isinstance(scheduled_job.trigger, IntervalTrigger) and not data_source.interval)
        ):
            self.scheduler.remove_job(scheduled_job.id)

        if isinstance(scheduled_job.trigger, CronTrigger):
            data_source_trigger = CronTrigger.from_crontab(data_source.crontab, timezone='UTC')
            data_source_trigger_value = str(data_source_trigger)
            scheduled_job_trigger_value = str(scheduled_job.trigger)
        elif isinstance(scheduled_job.trigger, IntervalTrigger):
            data_source_trigger = IntervalTrigger(
                start_date=data_source.start_time,
                end_date=data_source.end_time,
                **{data_source.interval_units: data_source.interval}
            )
            data_source_trigger_value = data_source_trigger.interval_length
            scheduled_job_trigger_value = scheduled_job.trigger.interval_length
        else:
            data_source_trigger_value = None
            scheduled_job_trigger_value = None

        if data_source_trigger_value != scheduled_job_trigger_value:
            self.scheduler.remove_job(scheduled_job.id)

        if not self.scheduler.get_job(scheduled_job.id) and \
                (data_source.crontab or data_source.interval):
            self.add_schedule(data_source)

    @staticmethod
    def load_data(data_source):
        """
        The load_data function is used to load data from a data source into the
        data warehouse. The function takes in a single argument, which is an object
        representing the data source that you want to load. This function will then
        call on the service's 'load_data' method, passing in the ID of your desired
        data source as an argument.

        :param data_source: Identify the data source that you want to load
        :return: None
        """

        data_source.refresh()

        if data_source.paused is True:
            logging.info(f'Data source {data_source.name} is paused: Skipping')
            return

        logging.info(f'Loading data source {data_source.name}')

        try:
            data_source.load_data()
            logging.info(f'Finished loading data source {data_source.name}')

        except Exception as e:
            logging.error(traceback.format_exc())
            logging.error(e)
